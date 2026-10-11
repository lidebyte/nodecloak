//! Portable login state. Browser APIs run against a closed, locally staged profile.
//! Network requests are fulfilled with an empty document, never with website scripts.
use crate::{engine, platform, profiles};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    time::{Duration, Instant},
};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

pub fn origin(v: &str) -> Result<String, String> {
    let u = reqwest::Url::parse(v).map_err(|_| "网站来源无效")?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || !matches!(u.path(), "" | "/")
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err("网站来源无效".into());
    }
    Ok(u.origin().ascii_serialization())
}
fn canonical_storage(state: &mut Value) -> Result<(), String> {
    let local = state["local"].as_array_mut().ok_or("网站存储无效")?;
    local.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
    let databases = state["databases"].as_array_mut().ok_or("网站存储无效")?;
    databases.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    for db in databases {
        let stores = db["stores"].as_array_mut().ok_or("网站存储无效")?;
        stores.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        for store in stores {
            store["indexes"]
                .as_array_mut()
                .ok_or("网站存储无效")?
                .sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        }
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: String,
    pub expires: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_key: Option<Partition>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Partition {
    pub top_level_site: String,
    pub has_cross_site_ancestor: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub origin: String,
    pub state: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema: u8,
    pub cookies: Vec<Cookie>,
    pub origins: Vec<Site>,
}
impl Snapshot {
    pub fn empty() -> Self {
        Self {
            schema: 1,
            cookies: Vec::new(),
            origins: Vec::new(),
        }
    }
    pub fn parse(v: Value) -> Result<Self, String> {
        if serde_json::to_vec(&v).map_err(|_| "登录数据无效")?.len() > 8 << 20 {
            return Err("登录数据超过 8 MiB，暂不支持同步".into());
        };
        let mut s: Self = serde_json::from_value(v).map_err(|_| "登录数据格式无效")?;
        if s.schema != 1 || s.cookies.len() > 10000 || s.origins.len() > 500 {
            return Err("登录数据版本或数量超过限制".into());
        }
        for c in &s.cookies {
            let domain = c.domain.strip_prefix('.').unwrap_or(&c.domain);
            if domain.is_empty()
                || domain.len() > 253
                || domain
                    .bytes()
                    .any(|b| !b.is_ascii_alphanumeric() && !b".-:".contains(&b))
                || c.name.len() > 4096
                || c.value.len() > 16384
                || c.path.len() > 4096
                || !c.path.starts_with('/')
                || !c.expires.is_finite()
                || !matches!(c.same_site.as_str(), "Lax" | "Strict" | "None")
            {
                return Err("Cookie 范围或内容无效".into());
            };
            if let Some(p) = &c.partition_key {
                if origin(&p.top_level_site)? != p.top_level_site {
                    return Err("Cookie 分区无效".into());
                }
            }
        }
        let mut seen = BTreeSet::new();
        for site in &s.origins {
            if origin(&site.origin)? != site.origin
                || !seen.insert(&site.origin)
                || !site.state.is_object()
            {
                return Err("网站存储来源无效".into());
            }
        }
        s.cookies.sort_by(|a, b| {
            (&a.domain, &a.path, &a.name, &a.partition_key).cmp(&(
                &b.domain,
                &b.path,
                &b.name,
                &b.partition_key,
            ))
        });
        s.origins.sort_by(|a, b| a.origin.cmp(&b.origin));
        for site in &mut s.origins {
            canonical_storage(&mut site.state)?;
        }
        Ok(s)
    }
    pub fn bytes(&self) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
        Ok(zeroize::Zeroizing::new(
            serde_json::to_vec(self).map_err(|_| "无法读取登录数据")?,
        ))
    }
}
fn copy_tree(src: &Path, dst: &Path, budget: &mut u64) -> Result<(), String> {
    profiles::guard_tree(src)?;
    std::fs::create_dir_all(dst).map_err(|_| "无法创建临时副本")?;
    for entry in std::fs::read_dir(src).map_err(|_| "无法读取浏览器副本")? {
        let e = entry.map_err(|_| "无法读取浏览器副本")?;
        let name = e.file_name();
        let n = name.to_string_lossy();
        if matches!(
            n.as_ref(),
            "parent.lock"
                | "lock"
                | ".parentlock"
                | "DevToolsActivePort"
                | "WebDriverBiDiServer.json"
                | "Cache"
                | "Code Cache"
                | "GPUCache"
                | "cache2"
                | "startupCache"
                | "ShaderCache"
                | "GrShaderCache"
                | "DawnCache"
                | "Crashpad"
        ) || n.starts_with("Singleton")
        {
            continue;
        }
        let from = e.path();
        profiles::guard_tree(&from)?;
        let to = dst.join(name);
        let meta = std::fs::symlink_metadata(&from).map_err(|_| "无法读取副本文件")?;
        if meta.is_dir() {
            copy_tree(&from, &to, budget)?
        } else if meta.is_file() {
            *budget = budget.checked_add(meta.len()).ok_or("副本过大")?;
            if *budget > 2 << 30 {
                return Err("副本数据超过 2 GiB，请先清理缓存再同步".into());
            };
            std::fs::copy(from, to).map_err(|_| "无法复制浏览器副本")?;
        } else {
            return Err("副本包含不支持的文件".into());
        }
    }
    Ok(())
}
fn disk_origins(browser: &str, path: &Path) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    if browser == "firefox" {
        let dir = path.join("storage/default");
        if dir.exists() {
            for e in std::fs::read_dir(dir).map_err(|_| "无法读取网站存储")? {
                let n = e
                    .map_err(|_| "网站存储无效")?
                    .file_name()
                    .to_string_lossy()
                    .into_owned();
                if !n.starts_with("https+++") && !n.starts_with("http+++") {
                    continue;
                };
                if n.contains('^') {
                    return Err("此副本包含分区网站存储，暂不支持完整迁移".into());
                };
                let u = n.replacen("+++", "://", 1).replace('+', ":");
                out.insert(origin(&u)?);
            }
        }
    } else {
        let dir = path.join("Default/IndexedDB");
        if dir.exists() {
            for e in std::fs::read_dir(dir).map_err(|_| "无法读取网站存储")? {
                let n = e
                    .map_err(|_| "网站存储无效")?
                    .file_name()
                    .to_string_lossy()
                    .into_owned();
                let key = n.split(".indexeddb.").next().unwrap_or("");
                if let Some((scheme, rest)) = key.split_once('_') {
                    if matches!(scheme, "http" | "https") {
                        if let Some((host, port)) = rest.rsplit_once('_') {
                            let u = if port == "0" {
                                format!("{scheme}://{host}")
                            } else {
                                format!("{scheme}://{host}:{port}")
                            };
                            out.insert(origin(&u)?);
                        }
                    }
                }
            }
        }
        let dir = path.join("Default/Local Storage/leveldb");
        if dir.join("CURRENT").exists() {
            use rusty_leveldb::{LdbIterator, Options, DB};
            let mut opts = Options::default();
            opts.create_if_missing = false;
            let mut db = DB::open(&dir, opts).map_err(|_| "无法枚举网站本地存储")?;
            let mut iter = db.new_iter().map_err(|_| "无法读取网站本地存储")?;
            let mut count = 0;
            while let Some((key, _)) = iter.next() {
                count += 1;
                if count > 100000 {
                    return Err("网站存储记录过多".into());
                };
                let raw = if key.starts_with(b"META:") {
                    &key[5..]
                } else if key.starts_with(b"_") {
                    key[1..].split(|b| *b == 0).next().unwrap_or(&[])
                } else {
                    continue;
                };
                if let Ok(s) = std::str::from_utf8(raw) {
                    if s.starts_with("https://") || s.starts_with("http://") {
                        let s = s.strip_suffix('/').unwrap_or(s);
                        if s.contains('^') {
                            return Err("此副本包含分区网站存储，暂不支持完整迁移".into());
                        };
                        out.insert(origin(s)?);
                    }
                }
            }
        }
    };
    if out.len() > 500 {
        return Err("副本网站数量超过限制".into());
    };
    Ok(out)
}
struct Browser {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    child: Child,
    next: u64,
    firefox: bool,
    target: String,
    session: Option<String>,
    closed: bool,
}
impl Browser {
    fn launch(browser: &str, path: &Path) -> Result<Self, String> {
        let exe = platform::browser_path(browser)?;
        let mut cmd = platform::command(&exe.to_string_lossy());
        cmd.stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null());
        if browser == "firefox" {
            cmd.args(["--headless", "--no-remote", "--profile"])
                .arg(path)
                .args(["--remote-debugging-port", "0", "about:blank"]);
        } else {
            cmd.arg(format!("--user-data-dir={}", path.display()))
                .args([
                    "--headless=new",
                    "--remote-debugging-address=127.0.0.1",
                    "--remote-debugging-port=0",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--disable-background-networking",
                    "--disable-extensions",
                    "--disable-sync",
                    "--proxy-server=http://127.0.0.1:9",
                    "--proxy-bypass-list=<-loopback>",
                    "about:blank",
                ]);
        }
        let mut child = cmd.spawn().map_err(|_| "无法启动隔离浏览器")?;
        let endpoint = match crate::browser_control::endpoint(browser, path) {
            Ok(v) => v,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let (mut socket, _) = match tungstenite::connect(endpoint.as_str()) {
            Ok(v) => v,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("无法连接隔离浏览器".into());
            }
        };
        if let MaybeTlsStream::Plain(s) = socket.get_mut() {
            s.set_read_timeout(Some(Duration::from_millis(100)))
                .map_err(|_| "无法设置超时")?;
            s.set_write_timeout(Some(Duration::from_secs(5)))
                .map_err(|_| "无法设置超时")?;
        }
        let mut b = Self {
            socket,
            child,
            next: 0,
            firefox: browser == "firefox",
            target: String::new(),
            session: None,
            closed: false,
        };
        if b.firefox {
            b.call(
                "session.new",
                json!({"capabilities":{"alwaysMatch":{"acceptInsecureCerts":false}}}),
                false,
            )?;
            let v = b.call("browsingContext.create", json!({"type":"tab"}), false)?;
            b.target = v["context"].as_str().ok_or("无法创建隔离页面")?.into();
            b.call(
                "session.subscribe",
                json!({"events":["network.beforeRequestSent"]}),
                false,
            )?;
            b.call(
                "network.addIntercept",
                json!({"phases":["beforeRequestSent"]}),
                false,
            )?;
        } else {
            let v = b.call("Target.createTarget", json!({"url":"about:blank"}), false)?;
            b.target = v["targetId"].as_str().ok_or("无法创建隔离页面")?.into();
            let v = b.call(
                "Target.attachToTarget",
                json!({"targetId":b.target,"flatten":true}),
                false,
            )?;
            b.session = Some(v["sessionId"].as_str().ok_or("无法连接隔离页面")?.into());
            b.call(
                "Network.setBypassServiceWorker",
                json!({"bypass":true}),
                true,
            )?;
            b.call(
                "Fetch.enable",
                json!({"patterns":[{"urlPattern":"*"}]}),
                true,
            )?;
        }
        Ok(b)
    }
    fn send(&mut self, method: &str, params: Value, target: bool) -> Result<u64, String> {
        self.next += 1;
        let mut v = json!({"id":self.next,"method":method,"params":params});
        if target {
            if let Some(s) = &self.session {
                v["sessionId"] = json!(s)
            }
        };
        self.socket
            .send(Message::Text(v.to_string().into()))
            .map_err(|_| "隔离浏览器连接中断")?;
        Ok(self.next)
    }
    fn call(&mut self, method: &str, params: Value, target: bool) -> Result<Value, String> {
        let id = self.send(method, params, target)?;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(40) {
            match self.socket.read() {
                Ok(Message::Text(text)) => {
                    if text.len() > 12 << 20 {
                        return Err("浏览器响应过大".into());
                    };
                    let v: Value = serde_json::from_str(&text).map_err(|_| "浏览器响应无效")?;
                    if v["id"] == id {
                        if v.get("error").is_some() || v["type"] == "error" {
                            return Err(format!("浏览器拒绝登录数据操作 {method}"));
                        };
                        return Ok(v["result"].clone());
                    }
                    if v["method"] == "Fetch.requestPaused" {
                        self.send("Fetch.fulfillRequest",json!({"requestId":v["params"]["requestId"],"responseCode":200,"responseHeaders":[{"name":"Content-Type","value":"text/html"},{"name":"Cache-Control","value":"no-store"}],"body":B64.encode(b"<!doctype html><title>NodeCloak sync</title>")}),true)?;
                    }
                    if v["method"] == "network.beforeRequestSent"
                        && v["params"]["isBlocked"] == true
                    {
                        self.send("network.provideResponse",json!({"request":v["params"]["request"]["request"],"statusCode":200,"headers":[{"name":"Content-Type","value":{"type":"string","value":"text/html"}}],"body":{"type":"base64","value":B64.encode(b"<!doctype html><title>NodeCloak sync</title>")}}),false)?;
                    }
                }
                Ok(Message::Close(_)) => return Err("隔离浏览器连接已关闭".into()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return Err("隔离浏览器连接中断".into()),
            }
        }
        Err(format!("浏览器登录数据操作 {method} 超时"))
    }
    fn eval(&mut self, expression: &str) -> Result<Value, String> {
        if self.firefox {
            let v=self.call("script.evaluate",json!({"expression":format!("(async()=>JSON.stringify(await ({expression})))()"),"target":{"context":self.target},"awaitPromise":true}),false)?;
            if v["type"] == "exception" {
                return Err("网站存储包含不可迁移的值，保留本机数据".into());
            };
            serde_json::from_str(v["result"]["value"].as_str().ok_or("网站存储响应无效")?)
                .map_err(|_| "网站存储响应无效".into())
        } else {
            let v = self.call(
                "Runtime.evaluate",
                json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),
                true,
            )?;
            if v.get("exceptionDetails").is_some() {
                return Err("网站存储包含不可迁移的值，保留本机数据".into());
            };
            Ok(v["result"]["value"].clone())
        }
    }
    fn site(&mut self, site: &str) -> Result<(), String> {
        if self.firefox {
            self.call(
                "browsingContext.navigate",
                json!({"context":self.target,"url":format!("{site}/"),"wait":"none"}),
                false,
            )?;
        } else {
            self.call("Page.navigate", json!({"url":format!("{site}/")}), true)?;
        }
        let start = Instant::now();
        loop {
            let r = self.eval("location.origin")?;
            if r == site {
                break;
            };
            if start.elapsed() > Duration::from_secs(15) {
                return Err("无法进入隔离网站存储".into());
            };
            std::thread::sleep(Duration::from_millis(50));
        }
        let code = format!(
            "(()=>{{{};return true;}})()",
            include_str!("session_codec.js")
        );
        self.eval(&code)?;
        Ok(())
    }
    fn cookies(&mut self) -> Result<Vec<Cookie>, String> {
        let v = self.call(
            if self.firefox {
                "storage.getCookies"
            } else {
                "Storage.getCookies"
            },
            json!({}),
            false,
        )?;
        let list = v["cookies"].as_array().ok_or("Cookie 响应无效")?;
        let mut result = Vec::new();
        for c in list {
            if c["partitionKeyOpaque"] == true {
                return Err("此副本包含不可迁移的 Cookie 分区".into());
            };
            if self.firefox && c["value"]["type"] != "string" {
                return Err("此副本包含不可迁移的二进制 Cookie".into());
            };
            result.push(Cookie {
                name: c["name"].as_str().ok_or("Cookie 无效")?.into(),
                value: if self.firefox {
                    c["value"]["value"].as_str()
                } else {
                    c["value"].as_str()
                }
                .ok_or("Cookie 无效")?
                .into(),
                domain: c["domain"].as_str().ok_or("Cookie 无效")?.into(),
                path: c["path"].as_str().ok_or("Cookie 无效")?.into(),
                secure: c["secure"] == true,
                http_only: c["httpOnly"] == true,
                same_site: match c["sameSite"].as_str().unwrap_or("None") {
                    "strict" | "Strict" => "Strict",
                    "lax" | "Lax" => "Lax",
                    _ => "None",
                }
                .into(),
                expires: if self.firefox {
                    c["expiry"].as_f64().unwrap_or(-1.)
                } else {
                    c["expires"].as_f64().unwrap_or(-1.)
                },
                partition_key: c
                    .get("partitionKey")
                    .filter(|v| v.is_object())
                    .map(|v| serde_json::from_value(v.clone()).map_err(|_| "Cookie 分区无效"))
                    .transpose()?,
            });
        }
        Ok(result)
    }
    fn set_cookies(&mut self, cookies: &[Cookie]) -> Result<(), String> {
        self.call(
            if self.firefox {
                "storage.deleteCookies"
            } else {
                "Storage.clearCookies"
            },
            json!({}),
            false,
        )?;
        if self.firefox {
            for c in cookies {
                if c.partition_key.is_some() {
                    return Err("Firefox 暂不支持导入此 Cookie 分区".into());
                };
                let mut v = json!({"name":c.name,"value":{"type":"string","value":c.value},"domain":c.domain,"path":c.path,"secure":c.secure,"httpOnly":c.http_only,"sameSite":c.same_site.to_lowercase()});
                if c.expires > 0. {
                    v["expiry"] = json!(c.expires.floor() as u64)
                };
                self.call("storage.setCookie", json!({"cookie":v}), false)?;
            }
        } else {
            let mut list = Vec::new();
            for c in cookies {
                let mut v = serde_json::to_value(c).map_err(|_| "Cookie 无效")?;
                if c.expires < 0. {
                    v.as_object_mut().unwrap().remove("expires");
                }
                list.push(v)
            }
            self.call("Storage.setCookies", json!({"cookies":list}), false)?;
        };
        Ok(())
    }
    fn close(&mut self) -> Result<(), String> {
        let _ = self.send(
            if self.firefox {
                "browser.close"
            } else {
                "Browser.close"
            },
            json!({}),
            false,
        );
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            if self
                .child
                .try_wait()
                .map_err(|_| "无法等待浏览器关闭")?
                .is_some()
            {
                self.closed = true;
                return Ok(());
            };
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("隔离浏览器未正常关闭，取消同步".into())
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
struct Stage {
    temp: tempfile::TempDir,
    path: PathBuf,
    firefox_prefs: Vec<(String, Option<Vec<u8>>)>,
}
impl Stage {
    fn new(browser: &str, profile: &Path) -> Result<Self, String> {
        profiles::guard_tree(profile)?;
        platform::ensure_profile_closed(profile)?;
        let parent = profile.parent().ok_or("副本目录无效")?;
        std::fs::create_dir_all(parent).map_err(|_| "无法创建副本目录")?;
        let temp = tempfile::Builder::new()
            .prefix(".nodecloak-sync-")
            .tempdir_in(parent)
            .map_err(|_| "无法创建临时副本")?;
        let path = temp.path().join("profile");
        if profile.exists() {
            copy_tree(profile, &path, &mut 0)?
        } else {
            std::fs::create_dir(&path).map_err(|_| "无法创建临时副本")?
        };
        let mut firefox_prefs = Vec::new();
        if browser == "firefox" {
            for name in ["user.js", "prefs.js"] {
                firefox_prefs.push((name.into(), std::fs::read(path.join(name)).ok()));
            }
            let mut user = firefox_prefs[0].1.clone().unwrap_or_default();
            user.extend_from_slice(b"\nuser_pref(\"network.proxy.type\",1);user_pref(\"network.proxy.http\",\"127.0.0.1\");user_pref(\"network.proxy.http_port\",9);user_pref(\"network.proxy.ssl\",\"127.0.0.1\");user_pref(\"network.proxy.ssl_port\",9);user_pref(\"network.proxy.no_proxies_on\",\"\");user_pref(\"network.proxy.allow_hijacking_localhost\",true);user_pref(\"dom.serviceWorkers.enabled\",false);user_pref(\"browser.sessionstore.resume_from_crash\",false);user_pref(\"browser.startup.page\",0);\n");
            engine::atomic_write(&path.join("user.js"), &user)?;
        }
        Ok(Self {
            temp,
            path,
            firefox_prefs,
        })
    }
    fn restore_prefs(&self) -> Result<(), String> {
        for (n, v) in &self.firefox_prefs {
            let p = self.path.join(n);
            if let Some(v) = v {
                engine::atomic_write(&p, v)?
            } else if p.exists() {
                std::fs::remove_file(p).map_err(|_| "无法恢复副本设置")?
            }
        }
        for n in ["DevToolsActivePort", "WebDriverBiDiServer.json"] {
            let p = self.path.join(n);
            if p.exists() {
                std::fs::remove_file(p).map_err(|_| "无法清理调试标记")?
            }
        }
        Ok(())
    }
    fn commit(self, profile: &Path) -> Result<(), String> {
        platform::ensure_profile_closed(profile)?;
        self.restore_prefs()?;
        let backup = self.temp.path().join("previous");
        let journal = journal_path(profile)?;
        let name = self
            .temp
            .path()
            .file_name()
            .ok_or("临时目录无效")?
            .to_string_lossy()
            .into_owned();
        engine::atomic_write(
            &journal,
            &serde_json::to_vec(&json!({"stage":name,"hadOriginal":profile.exists()}))
                .map_err(|_| "恢复记录无效")?,
        )?;
        // Keep the staged directory on failure/crash; recover() handles the two rename boundaries.
        let _keep = self.temp.keep();
        if profile.exists() {
            std::fs::rename(profile, &backup).map_err(|_| "无法备份当前副本")?
        };
        if std::fs::rename(&self.path, profile).is_err() {
            if backup.exists() {
                let _ = std::fs::rename(&backup, profile);
            };
            return Err("无法替换副本，已保留原数据".into());
        };
        std::fs::remove_file(journal).map_err(|_| "无法清理恢复记录")?;
        let stage = backup.parent().unwrap();
        profiles::guard_tree(stage)?;
        std::fs::remove_dir_all(stage).map_err(|_| "无法清理本机临时备份")?;
        Ok(())
    }
}
fn journal_path(profile: &Path) -> Result<PathBuf, String> {
    let name = profile
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("副本目录无效")?;
    Ok(profile.with_file_name(format!(".nodecloak-session-{name}.json")))
}
pub fn recover(profile: &Path) -> Result<(), String> {
    let journal = journal_path(profile)?;
    profiles::guard_tree(&journal)?;
    if !journal.exists() {
        return Ok(());
    };
    platform::ensure_profile_closed(profile)?;
    let v: Value =
        serde_json::from_slice(&std::fs::read(&journal).map_err(|_| "无法读取恢复记录")?)
            .map_err(|_| "恢复记录无效")?;
    let n = v["stage"].as_str().ok_or("恢复记录无效")?;
    if !n.starts_with(".nodecloak-sync-")
        || n.bytes()
            .any(|c| !c.is_ascii_alphanumeric() && !b".-_".contains(&c))
    {
        return Err("恢复目录无效".into());
    };
    let stage = profile.parent().unwrap().join(n);
    profiles::guard_tree(&stage)?;
    let backup = stage.join("previous");
    if !profile.exists() {
        let source = if backup.exists() {
            backup
        } else if v["hadOriginal"] == false && stage.join("profile").exists() {
            stage.join("profile")
        } else {
            return Err("本机副本恢复数据缺失".into());
        };
        std::fs::rename(&source, profile).map_err(|_| "无法恢复原副本")?
    };
    std::fs::remove_file(journal).map_err(|_| "无法清理恢复记录")?;
    if stage.exists() {
        std::fs::remove_dir_all(stage).map_err(|_| "无法清理恢复目录")?
    };
    Ok(())
}
pub fn export_profile(browser: &str, profile: &Path) -> Result<Snapshot, String> {
    recover(profile)?;
    let stage = Stage::new(browser, profile)?;
    let origins = disk_origins(browser, &stage.path)?;
    let mut b = Browser::launch(browser, &stage.path)?;
    let cookies = b.cookies()?;
    let mut sites = Vec::new();
    for o in origins {
        b.site(&o)?;
        let state = b.eval("__ncSessionCodec.exportOrigin()")?;
        sites.push(Site { origin: o, state });
    }
    b.close()?;
    Snapshot::parse(json!({"schema":1,"cookies":cookies,"origins":sites}))
}
pub fn restore_profile(
    browser: &str,
    profile: &Path,
    snapshot: &Snapshot,
) -> Result<Snapshot, String> {
    let snapshot = Snapshot::parse(serde_json::to_value(snapshot).map_err(|_| "登录数据无效")?)?;
    recover(profile)?;
    let stage = Stage::new(browser, profile)?;
    let mut origins = disk_origins(browser, &stage.path)?;
    origins.extend(snapshot.origins.iter().map(|s| s.origin.clone()));
    let mut b = Browser::launch(browser, &stage.path)?;
    for o in origins {
        b.site(&o)?;
        let empty = json!({"local":[],"databases":[]});
        let state = snapshot
            .origins
            .iter()
            .find(|s| s.origin == o)
            .map(|s| &s.state)
            .unwrap_or(&empty);
        let expression = format!(
            "__ncSessionCodec.importOrigin({})",
            serde_json::to_string(state).map_err(|_| "网站存储无效")?
        );
        b.eval(&expression)?;
    }
    b.set_cookies(&snapshot.cookies)?;
    let cookies = b.cookies()?;
    for c in snapshot
        .cookies
        .iter()
        .filter(|c| c.expires < 0. || c.expires > chrono::Utc::now().timestamp() as f64)
    {
        if !cookies.iter().any(|v| {
            v.name == c.name
                && v.value == c.value
                && v.domain == c.domain
                && v.path == c.path
                && v.secure == c.secure
                && v.http_only == c.http_only
                && v.same_site == c.same_site
                && v.partition_key == c.partition_key
        }) {
            return Err("浏览器未保留全部 Cookie，取消导入并保留本机数据".into());
        }
    }
    let mut sites = Vec::new();
    for site in &snapshot.origins {
        b.site(&site.origin)?;
        let mut state = b.eval("__ncSessionCodec.exportOrigin()")?;
        canonical_storage(&mut state)?;
        if state != site.state {
            return Err("网站存储写入复检不一致，取消导入并保留本机数据".into());
        };
        sites.push(Site {
            origin: site.origin.clone(),
            state,
        });
    }
    let actual = Snapshot::parse(json!({"schema":1,"cookies":cookies,"origins":sites}))?;
    b.close()?;
    stage.commit(profile)?;
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn interrupted_replacement_recovers_at_each_rename_boundary() {
        for phase in 0..4 {
            let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
            let profile = dir.path().join("profile");
            let stage = dir.path().join(".nodecloak-sync-test");
            std::fs::create_dir(&stage).unwrap();
            let target = match phase {
                0 => {
                    std::fs::create_dir(&profile).unwrap();
                    profile.clone()
                }
                1 => {
                    std::fs::create_dir(stage.join("previous")).unwrap();
                    stage.join("previous")
                }
                2 => {
                    std::fs::create_dir(&profile).unwrap();
                    std::fs::create_dir(stage.join("previous")).unwrap();
                    profile.clone()
                }
                _ => {
                    std::fs::create_dir(stage.join("profile")).unwrap();
                    stage.join("profile")
                }
            };
            std::fs::write(target.join("sentinel"), b"preserved").unwrap();
            engine::atomic_write(
                &journal_path(&profile).unwrap(),
                &serde_json::to_vec(
                    &json!({"stage":".nodecloak-sync-test","hadOriginal":phase!=3}),
                )
                .unwrap(),
            )
            .unwrap();
            recover(&profile).unwrap();
            assert_eq!(
                std::fs::read(profile.join("sentinel")).unwrap(),
                b"preserved"
            );
            assert!(!stage.exists());
            assert!(!journal_path(&profile).unwrap().exists());
        }
    }
    #[test]
    fn origin_parser_rejects_network_credentials_paths_and_nonweb_schemes() {
        for v in [
            "file:///C:/secret",
            "https://user:pass@example.com",
            "https://example.com/path",
            "https://example.com/?x=1",
            "javascript:alert(1)",
        ] {
            assert!(origin(v).is_err(), "{v}")
        }
        assert_eq!(
            origin("https://example.com:443/"),
            Ok("https://example.com".into())
        );
        assert_eq!(
            origin("http://localhost:18443"),
            Ok("http://localhost:18443".into())
        );
    }
    #[test]
    fn snapshot_validation_rejects_unknown_fields_and_unsafe_cookie_scope() {
        let mut v = json!({"schema":1,"cookies":[],"origins":[]});
        v["executablePath"] = json!("C:/evil.exe");
        assert!(Snapshot::parse(v).is_err());
        let v = json!({"schema":1,"cookies":[{"name":"x","value":"secret","domain":"../../evil","path":"/","secure":true,"httpOnly":true,"sameSite":"Lax","expires":-1}],"origins":[]});
        assert!(Snapshot::parse(v).is_err());
    }
    #[test]
    fn importing_cannot_touch_an_open_browser() {
        let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let profile = dir.path().join("profile");
        std::fs::create_dir_all(&profile).unwrap();
        #[cfg(windows)]
        {
            let _lock = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .share_mode(0)
                .open(profile.join("parent.lock"))
                .unwrap();
            assert!(restore_profile("firefox", &profile, &Snapshot::empty()).is_err());
        }
    }
    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;
    #[test]
    #[ignore = "launches installed browsers against synthetic, temporary profiles only"]
    fn installed_browsers_roundtrip_cookie_localstorage_and_indexeddb() {
        for browser in ["chrome", "edge", "firefox"] {
            let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
            let profile = dir.path().join("synthetic");
            let token = format!("synthetic-{}", uuid::Uuid::new_v4());
            let packed = |v: Value| json!({"root":v,"nodes":[]});
            let input=Snapshot::parse(json!({"schema":1,"cookies":[{"name":"nc_synthetic","value":token,"domain":"localhost","path":"/","secure":true,"httpOnly":true,"sameSite":"Lax","expires":chrono::Utc::now().timestamp()+604800}],"origins":[{"origin":"https://localhost:18443","state":{"local":[["session",token]],"databases":[{"name":"nc-synthetic","version":1,"stores":[{"name":"kv","keyPath":null,"autoIncrement":false,"indexes":[],"rows":[[packed(json!("session")),packed(json!(token))]]}]}]}}]})).unwrap();
            restore_profile(browser, &profile, &input).unwrap_or_else(|e| panic!("{browser}: {e}"));
            let exported =
                export_profile(browser, &profile).unwrap_or_else(|e| panic!("{browser}: {e}"));
            assert!(
                exported.cookies.iter().any(|c| c.name == "nc_synthetic"
                    && c.value == token
                    && c.http_only
                    && c.secure),
                "{browser}: cookie missing"
            );
            assert_eq!(exported.origins.len(), 1, "{browser}: origin discovery");
            assert_eq!(
                exported.origins[0].state, input.origins[0].state,
                "{browser}: storage"
            );
            let target = dir.path().join("second-profile");
            restore_profile(browser, &target, &exported)
                .unwrap_or_else(|e| panic!("{browser}: {e}"));
            let second = export_profile(browser, &target).unwrap();
            assert_eq!(
                second.bytes().unwrap().as_slice(),
                exported.bytes().unwrap().as_slice(),
                "{browser}: restart persistence"
            );
        }
    }
}
