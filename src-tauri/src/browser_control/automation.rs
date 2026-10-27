use super::*;
use crate::mcp::policy::Grant;
#[derive(Clone)]
pub(super) struct Cleanup {
    context: String,
    realm: String,
    handle: Option<String>,
    mouse: Option<(f64, f64)>,
}

impl Runtime {
    pub(crate) fn automation(
        &self,
        id: &str,
        tool: &str,
        args: Value,
        grant: Grant,
        cancel: Arc<AtomicBool>,
    ) -> Result<Value, String> {
        let sender = {
            let m = self.0.lock().map_err(|_| "CONTROL_LOCK_FAILED")?;
            let c = m
                .get(id)
                .filter(|c| c.alive.load(Ordering::SeqCst))
                .ok_or("RESTART_REQUIRED: 关闭后通过 MCP 重新启动此副本")?;
            if !c.automation {
                return Err("RESTART_REQUIRED: 此副本仅应用指纹和区域设置，请关闭后通过 MCP 启动以启用自动化".into());
            }
            c.sender.clone()
        };
        let (tx, rx) = mpsc::channel();
        sender
            .send(Request::Automation(
                tool.into(),
                args,
                grant,
                cancel.clone(),
                tx,
            ))
            .map_err(|_| "CONTROLLER_DISCONNECTED")?;
        match rx.recv_timeout(Duration::from_secs(45)) {
            Ok(result) => result,
            Err(_) => {
                cancel.store(true, Ordering::SeqCst);
                Err("BROWSER_OPERATION_TIMEOUT".into())
            }
        }
    }
}
impl BrowserControl {
    pub(super) fn remember_dialog(&mut self, event: &Value) {
        let key = if self.firefox {
            event["params"]["context"].as_str()
        } else {
            event["sessionId"].as_str()
        };
        if let Some(key) = key {
            let mut dialog = event["params"].clone();
            dialog["prompt_id"] = json!(uuid::Uuid::new_v4().to_string());
            self.automation_dialogs.insert(key.to_string(), dialog);
        }
    }

    pub(super) fn forget_dialog(&mut self, event: &Value) {
        let key = if self.firefox {
            event["params"]["context"].as_str()
        } else {
            event["sessionId"].as_str()
        };
        if let Some(key) = key {
            self.automation_dialogs.remove(key);
        }
    }
    pub(super) fn cleanup_closed_dialogs(&mut self) {
        let pending = std::mem::take(&mut self.automation_cleanup);
        for cleanup in pending {
            if !self.automation_dialogs.contains_key(&cleanup.context) {
                let _ = self.finish_input(cleanup);
            } else {
                self.automation_cleanup.push(cleanup);
            }
        }
    }
    fn finish_input(&mut self, mut cleanup: Cleanup) -> Result<(), String> {
        let never_cancel = AtomicBool::new(false);
        if self.firefox {
            let result = self.command(
                "input.releaseActions",
                json!({"context":cleanup.context}),
                None,
                &never_cancel,
            )?;
            if result["dialog_open"] == true {
                self.automation_cleanup.push(cleanup);
                return Ok(());
            }
            if let Some(handle) = cleanup.handle.take() {
                let _=self.command("script.disown",json!({"handles":[handle],"target":{"context":cleanup.context,"sandbox":cleanup.realm}}),None,&never_cancel);
            }
        } else {
            if let Some((x, y)) = cleanup.mouse.take() {
                let result = self.command(
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseReleased","x":x,"y":y,"button":"left","clickCount":1}),
                    Some(&cleanup.context),
                    &never_cancel,
                )?;
                if result["dialog_open"] == true {
                    self.automation_cleanup.push(cleanup);
                    return Ok(());
                }
            }
            if let Some(object) = cleanup.handle.take() {
                let _ = self.command(
                    "Runtime.releaseObject",
                    json!({"objectId":object}),
                    Some(&cleanup.context),
                    &never_cancel,
                );
            }
        }
        Ok(())
    }
    fn secure_drain(&mut self) -> Result<(), String> {
        if let Err(error) = self.drain() {
            self.automation_fatal = Some(error.clone());
            return Err(error);
        }
        Ok(())
    }

    fn command(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        if cancel.load(Ordering::SeqCst) {
            return Err("CANCELLED".into());
        }
        self.protocol.next += 1;
        let id = self.protocol.next;
        let mut message = json!({"id":id,"method":method,"params":params});
        if let Some(s) = session {
            message["sessionId"] = json!(s);
        }
        self.protocol.navigation = Some((id, None));
        self.protocol
            .socket
            .send(Message::Text(message.to_string().into()))
            .map_err(|_| "CONTROLLER_DISCONNECTED")?;
        let result = (|| {
            let start = Instant::now();
            loop {
                self.secure_drain()?;
                if let Some((_, Some(reply))) = &mut self.protocol.navigation {
                    let reply = reply.take();
                    if reply.get("error").is_some() || reply["type"] == "error" {
                        return Err(format!(
                            "BROWSER_REJECTED: {}",
                            reply["error"]["message"]
                                .as_str()
                                .or(reply["message"].as_str())
                                .unwrap_or(method)
                        ));
                    }
                    let result = reply["result"].clone();
                    if result["type"] == "exception" || result.get("exceptionDetails").is_some() {
                        return Err(format!(
                            "PAGE_OPERATION_FAILED: {}",
                            result["exceptionDetails"]["exception"]["description"]
                                .as_str()
                                .or(result["exceptionDetails"]["text"].as_str())
                                .unwrap_or("Read a fresh snapshot")
                        ));
                    }
                    return Ok(result);
                }
                if cancel.load(Ordering::SeqCst) {
                    return Err("CANCELLED".into());
                }
                if start.elapsed() > Duration::from_secs(18) {
                    return Err(format!("BROWSER_OPERATION_TIMEOUT: {method}"));
                }
                if let Some(event) = self.protocol.read()? {
                    if event["method"] == "Page.javascriptDialogClosed"
                        || event["method"] == "browsingContext.userPromptClosed"
                    {
                        self.forget_dialog(&event);
                        continue;
                    }
                    if event["method"] == "Page.javascriptDialogOpening"
                        || event["method"] == "browsingContext.userPromptOpened"
                    {
                        let own = if self.firefox {
                            event["params"]["context"] == message["params"]["context"]
                                || event["params"]["context"]
                                    == message["params"]["target"]["context"]
                        } else {
                            event["sessionId"].as_str() == session
                        };
                        self.remember_dialog(&event);
                        let interaction = [
                            "Runtime.evaluate",
                            "script.evaluate",
                            "Input.dispatchMouseEvent",
                            "input.performActions",
                            "input.releaseActions",
                            "DOM.setFileInputFiles",
                            "input.setFiles",
                            "Page.navigate",
                            "browsingContext.navigate",
                        ]
                        .contains(&method);
                        if own && interaction {
                            return Ok(json!({"dialog_open":true}));
                        }
                        continue;
                    }
                    if event["id"] == id {
                        self.protocol.navigation.as_mut().unwrap().1 = Some(event);
                    } else if event.get("id").is_none() {
                        self.protocol.events.push_back(event);
                    }
                }
            }
        })();
        self.protocol.navigation = None;
        result
    }
    pub(super) fn tree(&mut self, cancel: &AtomicBool) -> Result<Vec<Value>, String> {
        if self.firefox {
            let v = self.command("browsingContext.getTree", json!({}), None, cancel)?;
            let mut out = vec![];
            fn visit(v: &Value, tab: &str, out: &mut Vec<Value>) {
                let id = v["context"].as_str().unwrap_or("");
                let tab = if tab.is_empty() { id } else { tab };
                out.push(json!({"tab_id":tab,"frame_id":id,"url":v["url"],"top_level":id==tab,"opener":v["originalOpener"]}));
                for c in v["children"].as_array().into_iter().flatten() {
                    visit(c, tab, out);
                }
            }
            for c in v["contexts"].as_array().into_iter().flatten() {
                visit(c, "", &mut out);
            }
            Ok(out)
        } else {
            let v = self.command("Target.getTargets", json!({}), None, cancel)?;
            self.secure_drain()?;
            let mut out = vec![];
            for t in v["targetInfos"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t["type"] == "page")
            {
                let id = t["targetId"].as_str().ok_or("TAB_NOT_FOUND")?;
                out.push(json!({"tab_id":id,"frame_id":id,"url":t["url"],"title":t["title"],"top_level":true,"opener":t["openerId"]}));
                if let Some(session) = self.targets.get(id).cloned() {
                    // A rendererless/initializing tab still belongs in the tab
                    // list, but cannot provide a frame tree until navigation.
                    if self.protocol.document_pending(&session) { continue; }
                    let tree = match self.command("Page.getFrameTree", json!({}), Some(&session), cancel) {
                        Ok(tree) => tree,
                        // A tab may close after Target.getTargets. Its entry was just
                        // appended, so omit only that vanished target and retain live tabs.
                        Err(error) if closed_target_error(&error) => { out.pop(); continue; }
                        Err(error) => return Err(error),
                    };
                    fn visit(v: &Value, tab: &str, out: &mut Vec<Value>) {
                        let f = &v["frame"];
                        out.push(json!({"tab_id":tab,"frame_id":f["id"],"url":f["url"],"top_level":false}));
                        for c in v["childFrames"].as_array().into_iter().flatten() {
                            visit(c, tab, out);
                        }
                    }
                    for c in tree["frameTree"]["childFrames"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        visit(c, id, &mut out);
                    }
                }
            }
            // OOP iframe targets are omitted from the top page's frame tree on Chromium.
            let targets: Vec<_> = v["targetInfos"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t["type"] == "iframe")
                .cloned()
                .collect();
            for _ in 0..8 {
                let mut added = false;
                for t in &targets {
                    let id = t["targetId"].as_str().unwrap_or("");
                    if out.iter().any(|f| f["frame_id"] == id) {
                        continue;
                    }
                    let Some(session) = self.targets.get(id).cloned() else {
                        continue;
                    };
                    if self.protocol.document_pending(&session) { continue; }
                    let tree = match self.command("Page.getFrameTree", json!({}), Some(&session), cancel) {
                        Ok(tree) => tree,
                        Err(error) if closed_target_error(&error) => continue,
                        Err(error) => return Err(error),
                    };
                    let f = &tree["frameTree"]["frame"];
                    let parent = f["parentId"].as_str();
                    let mut owner = parent.and_then(|p| {
                        out.iter()
                            .find(|x| x["frame_id"] == p)
                            .map(|x| x["tab_id"].clone())
                    });
                    if owner.is_none() {
                        for root in out.clone().iter().filter(|x| x["top_level"] == true) {
                            if let Some(root_session) = self
                                .targets
                                .get(root["tab_id"].as_str().unwrap_or(""))
                                .cloned()
                            {
                                if self
                                    .command(
                                        "DOM.getFrameOwner",
                                        json!({"frameId":id}),
                                        Some(&root_session),
                                        cancel,
                                    )
                                    .is_ok()
                                {
                                    owner = Some(root["tab_id"].clone());
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(tab) = owner {
                        out.push(
                            json!({"tab_id":tab,"frame_id":id,"url":f["url"],"top_level":false}),
                        );
                        added = true;
                    }
                }
                if !added {
                    break;
                }
            }
            Ok(out)
        }
    }
    fn target(
        &mut self,
        a: &Value,
        g: &Grant,
        cancel: &AtomicBool,
    ) -> Result<(String, String), String> {
        let tab = a["tab_id"].as_str().ok_or("TAB_REQUIRED")?;
        let frame = a["frame_id"].as_str().unwrap_or(tab);
        let tree = self.tree(cancel)?;
        let root = tree
            .iter()
            .find(|x| x["tab_id"] == tab && x["top_level"] == true)
            .ok_or("TAB_NOT_FOUND")?;
        g.origin(root["url"].as_str().unwrap_or(""))?;
        let selected = tree
            .iter()
            .find(|x| x["tab_id"] == tab && x["frame_id"] == frame)
            .ok_or("FRAME_NOT_IN_TAB")?;
        g.origin(selected["url"].as_str().unwrap_or(""))?;
        let context = if self.firefox {
            frame.into()
        } else {
            self.targets
                .get(frame)
                .or_else(|| self.targets.get(tab))
                .cloned()
                .ok_or("TAB_NOT_CONTROLLED")?
        };
        if !self.firefox {
            self.command("Page.enable", json!({}), Some(&context), cancel)?;
        }
        let actual_frame = if !self.firefox && frame == tab {
            let result = self.command("Page.getFrameTree", json!({}), Some(&context), cancel)?;
            result["frameTree"]["frame"]["id"]
                .as_str()
                .ok_or("FRAME_NOT_FOUND")?
                .into()
        } else {
            frame.into()
        };
        Ok((context, actual_frame))
    }
    fn evaluate(
        &mut self,
        context: &str,
        frame: &str,
        g: &Grant,
        args: &Value,
        node: bool,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        let realm = format!("nodecloak-{}", g.id);
        let source = include_str!("../mcp/page.js");
        let expression=format!("(()=>{{const allowed={};if(location.href!=='about:blank'&&(!['http:','https:'].includes(location.protocol)||(allowed.length&&!allowed.includes(location.origin))))throw Error('ORIGIN_NOT_AUTHORIZED');return ({source})({});}})()",json!(g.origins),args);
        if self.firefox {
            let result=self.command("script.evaluate",json!({"expression":expression,"target":{"context":context,"sandbox":realm},"awaitPromise":true,"resultOwnership":if node{"root"}else{"none"}}),None,cancel)?;
            if result["dialog_open"] == true {
                return Ok(result);
            }
            if node {
                Ok(result["result"].clone())
            } else {
                decode_remote(&result["result"])
            }
        } else {
            let world = self.command(
                "Page.createIsolatedWorld",
                json!({"frameId":frame,"worldName":realm,"grantUniveralAccess":false}),
                Some(context),
                cancel,
            )?;
            let result=self.command("Runtime.evaluate",json!({"expression":expression,"contextId":world["executionContextId"],"awaitPromise":true,"returnByValue":!node}),Some(context),cancel)?;
            if result["dialog_open"] == true {
                return Ok(result);
            }
            if node {
                Ok(result["result"].clone())
            } else {
                Ok(result["result"]["value"].clone())
            }
        }
    }
    pub(super) fn automate(
        &mut self,
        tool: &str,
        mut a: Value,
        g: &Grant,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        if !self.plan.automation {
            return Err("AUTOMATION_NOT_ENABLED".into());
        }
        g.authorize(
            if tool == "__preview" || tool == "__preview_dialog" {
                "read"
            } else {
                crate::mcp::catalog_permission(tool)?
            },
            a["profile_id"].as_str(),
            chrono::Utc::now().timestamp(),
        )?;
        if tool == "tabs_list" || tool == "frames_list" {
            let list = self.tree(cancel)?;
            let filtered: Vec<_> = list
                .into_iter()
                .filter(|v| {
                    g.origin(v["url"].as_str().unwrap_or("")).is_ok()
                        && (if tool == "tabs_list" {
                            v["top_level"] == true
                        } else {
                            v["tab_id"] == a["tab_id"]
                        })
                })
                .collect();
            return Ok(json!({"items":filtered}));
        }
        if tool == "tabs_open" {
            let url = a["url"].as_str().ok_or("URL_REQUIRED")?.to_string();
            g.origin(&url)?;
            let tab = if self.firefox {
                let reference = self
                    .tree(cancel)?
                    .into_iter()
                    .find(|v| v["top_level"] == true)
                    .and_then(|v| v["tab_id"].as_str().map(str::to_owned))
                    .ok_or("TAB_NOT_FOUND")?;
                self.command(
                    "browsingContext.create",
                    json!({"type":"tab","referenceContext":reference,"background":true}),
                    None,
                    cancel,
                )?["context"]
                    .as_str()
                    .ok_or("TAB_CREATE_FAILED")?
                    .to_owned()
            } else {
                self.command(
                    "Target.createTarget",
                    json!({"url":"about:blank"}),
                    None,
                    cancel,
                )?["targetId"]
                    .as_str()
                    .ok_or("TAB_CREATE_FAILED")?
                    .to_owned()
            };
            let start = Instant::now();
            while !self.firefox && !self.targets.contains_key(&tab) {
                if let Err(error) = self.secure_drain() {
                    return Ok(json!({"tab_id":tab,"initialization_error":error}));
                }
                if start.elapsed() > Duration::from_secs(5) {
                    return Ok(json!({"tab_id":tab,"initialization_error":"TAB_NOT_CONTROLLED"}));
                }
                match self.protocol.read() {
                    Ok(Some(e)) => self.protocol.events.push_back(e),
                    Ok(None) => {}
                    Err(error) => return Ok(json!({"tab_id":tab,"initialization_error":error})),
                }
            }
            a["tab_id"] = json!(tab);
            let result = self.automate("page_navigate", a.clone(), g, cancel);
            // Always return the new tab ID so it remains owned by this client/task even
            // when navigation fails or is cancelled after creation.
            return Ok(match result {
                Ok(value) => json!({"tab_id":tab,"navigation":value}),
                Err(error) => json!({"tab_id":tab,"navigation_error":error}),
            });
        }
        if tool == "downloads_list" {
            let dir = download_dir(g, a["profile_id"].as_str().unwrap_or(""))?;
            let mut files = vec![];
            for entry in std::fs::read_dir(&dir)
                .map_err(|_| "DOWNLOAD_DIRECTORY_UNAVAILABLE")?
                .take(200)
            {
                let e = entry.map_err(|_| "DOWNLOAD_READ_FAILED")?;
                let canonical =
                    std::fs::canonicalize(e.path()).map_err(|_| "DOWNLOAD_READ_FAILED")?;
                if canonical.starts_with(&dir) && canonical.is_file() {
                    let m = e.metadata().map_err(|_| "DOWNLOAD_READ_FAILED")?;
                    files.push(json!({"name":e.file_name().to_string_lossy(),"bytes":m.len(),"path":canonical}));
                }
            }
            return Ok(json!({"files":files}));
        }
        if tool == "page_wait" {
            let end = Instant::now()
                + Duration::from_millis(a["timeout_ms"].as_u64().unwrap_or(5000).min(15000));
            loop {
                g.authorize(
                    "page_read",
                    a["profile_id"].as_str(),
                    chrono::Utc::now().timestamp(),
                )?;
                if cancel.load(Ordering::SeqCst) {
                    return Err("CANCELLED".into());
                }
                let check = (|| {
                    let (c, f) = self.target(&a, g, cancel)?;
                    self.evaluate(
                        &c,
                        &f,
                        g,
                        &json!({"action":"wait","text":a["text"].as_str().unwrap_or("")}),
                        false,
                        cancel,
                    )
                })();
                match check {
                    Ok(v)
                        if (if a.get("text").is_some() {
                            v["found"] == true
                        } else {
                            v["ready"] == true
                        }) =>
                    {
                        return Ok(json!({"ready":true}))
                    }
                    Err(e) if e.contains("ORIGIN_NOT_AUTHORIZED") || e.contains("CANCELLED") => {
                        return Err(e)
                    }
                    _ => {}
                }
                if Instant::now() >= end {
                    return Err("WAIT_TIMEOUT".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        if tool == "page_dialog" || tool == "__preview_dialog" {
            self.secure_drain()?;
            let tab = a["tab_id"].as_str().ok_or("TAB_REQUIRED")?;
            let context = if self.firefox {
                tab.to_string()
            } else {
                self.targets.get(tab).cloned().ok_or("TAB_NOT_FOUND")?
            };
            let dialog = self
                .automation_dialogs
                .get(&context)
                .cloned()
                .ok_or("DIALOG_NOT_FOUND")?;
            if let Some(url) = dialog["url"].as_str() {
                g.origin(url)?;
            } else if self.firefox {
                for item in self.tree(cancel)?.iter().filter(|x| x["tab_id"] == tab) {
                    g.origin(item["url"].as_str().unwrap_or(""))?;
                }
            }
            if tool == "__preview_dialog" {
                return Ok(
                    json!({"prompt_id":dialog["prompt_id"],"type":dialog["type"],"message":dialog["message"].as_str().unwrap_or("").chars().take(500).collect::<String>()}),
                );
            }
            if a.get("_expected_target").is_some()
                && a["_expected_target"]["prompt_id"] != dialog["prompt_id"]
            {
                return Err("DIALOG_CHANGED: request approval again".into());
            }
            self.claim_downloads(g,a["profile_id"].as_str().unwrap_or(""),tab,cancel)?;
            if self.firefox {
                self.command("browsingContext.handleUserPrompt",json!({"context":context,"accept":a["accept"],"userText":a["text"].as_str().unwrap_or("")}),None,cancel)?;
            } else {
                self.command(
                    "Page.handleJavaScriptDialog",
                    json!({"accept":a["accept"],"promptText":a["text"].as_str().unwrap_or("")}),
                    Some(&context),
                    cancel,
                )?;
            }
            self.automation_dialogs.remove(&context);
            self.cleanup_closed_dialogs();
            return Ok(json!({"handled":true,"dialog_open":!self.automation_dialogs.is_empty()}));
        }
        if tool == "page_navigate" { g.origin(a["url"].as_str().ok_or("URL_REQUIRED")?)?; }
        let (context, frame) = self.target(&a, g, cancel)?;
        if tool == "__preview" {
            a["action"] = json!(if a.get("element_ref").is_some() {
                "describe"
            } else {
                "state"
            });
            return self.evaluate(&context, &frame, g, &a, false, cancel);
        }
        if let Some(expected) = a.get("_expected_target") {
            let mut check = a.clone();
            check["action"] = json!(if a.get("element_ref").is_some() {
                "describe"
            } else {
                "state"
            });
            let current = self.evaluate(&context, &frame, g, &check, false, cancel)?;
            if current["url"] != expected["url"] || current["epoch"] != expected["epoch"] {
                return Err("PAGE_CHANGED: request approval again".into());
            }
            for key in ["tag", "type", "name", "field", "href", "form_action"] {
                if current[key] != expected[key] {
                    return Err("TARGET_CHANGED: request approval again".into());
                }
            }
        }
        if tool == "tabs_close" {
            if a.get("frame_id").is_some() && a["frame_id"] != a["tab_id"] {
                return Err("TOP_LEVEL_TAB_REQUIRED".into());
            }
            if self.firefox {
                self.command(
                    "browsingContext.close",
                    json!({"context":context}),
                    None,
                    cancel,
                )?;
            } else {
                self.command(
                    "Target.closeTarget",
                    json!({"targetId":a["tab_id"]}),
                    None,
                    cancel,
                )?;
            }
            // A closing target can disappear while enumerating its frames. The idle
            // loop retries restoration; the successful close itself stays successful.
            let _ = self.finish_closed_download_tab(a["tab_id"].as_str().unwrap_or(""),cancel);
            return Ok(json!({"closed":true}));
        }
        if crate::mcp::catalog_permission(tool).ok() == Some("page_write") || tool == "page_upload" {
            self.claim_downloads(g,a["profile_id"].as_str().unwrap_or(""),a["tab_id"].as_str().ok_or("TAB_REQUIRED")?,cancel)?;
        }
        if tool == "page_navigate" {
            let url = a["url"].as_str().ok_or("URL_REQUIRED")?;
            g.origin(url)?;
            let result = if self.firefox {
                self.command(
                    "browsingContext.navigate",
                    json!({"context":context,"url":url,"wait":"none"}),
                    None,
                    cancel,
                )?
            } else {
                self.command(
                    "Page.navigate",
                    json!({"url":url,"frameId":frame}),
                    Some(&context),
                    cancel,
                )?
            };
            if let Some(e) = result["errorText"].as_str().filter(|s| !s.is_empty()) {
                return Err(format!("NAVIGATION_FAILED: {e}"));
            }
            return Ok(json!({"navigation_started":true}));
        }
        if tool == "page_screenshot" {
            if a.get("frame_id").is_some() && a["frame_id"] != a["tab_id"] {
                return Err("TOP_LEVEL_TAB_REQUIRED".into());
            }
            let before = self.tree(cancel)?;
            for item in before.iter().filter(|v| v["tab_id"] == a["tab_id"]) {
                g.origin(item["url"].as_str().unwrap_or(""))?;
            }
            let result = if self.firefox {
                self.command(
                    "browsingContext.captureScreenshot",
                    json!({"context":context,"origin":"viewport","format":{"type":"image/png"}}),
                    None,
                    cancel,
                )?
            } else {
                self.command(
                    "Page.captureScreenshot",
                    json!({"format":"png","captureBeyondViewport":false}),
                    Some(&context),
                    cancel,
                )?
            };
            self.target(&a, g, cancel)?;
            let after = self.tree(cancel)?;
            let relevant = |list: Vec<Value>| {
                let mut v: Vec<String> = list
                    .into_iter()
                    .filter(|v| v["tab_id"] == a["tab_id"])
                    .map(|v| v.to_string())
                    .collect();
                v.sort();
                v
            };
            if relevant(before) != relevant(after) {
                return Err("PAGE_CHANGED_DURING_SCREENSHOT".into());
            }
            let data = result["data"].as_str().ok_or("SCREENSHOT_FAILED")?;
            if data.len() > 8_000_000 {
                return Err("SCREENSHOT_TOO_LARGE".into());
            }
            return Ok(json!({"mime_type":"image/png","image_base64":data}));
        }
        if tool == "page_click" {
            // Native pointer input needs the selected top-level browsing context active.
            // Edge startup/extension tabs can otherwise consume focus while acknowledging input.
            if self.firefox {
                self.command(
                    "browsingContext.activate",
                    json!({"context":a["tab_id"]}),
                    None,
                    cancel,
                )?;
            } else {
                // The selected frame can have its own OOPIF session. Only the
                // top-level page session may activate the tab; native input below
                // still uses the selected frame session and its coordinates.
                let tab_session = self.targets
                    .get(a["tab_id"].as_str().ok_or("TAB_REQUIRED")?)
                    .cloned().ok_or("TAB_NOT_CONTROLLED")?;
                self.command("Page.bringToFront", json!({}), Some(&tab_session), cancel)?;
            }
            a["action"] = json!("node");
            let node = self.evaluate(&context, &frame, g, &a, true, cancel)?;
            let mut cleanup = Cleanup {
                context: context.clone(),
                realm: format!("nodecloak-{}", g.id),
                handle: None,
                mouse: None,
            };
            let result = if self.firefox {
                let shared = node["sharedId"].as_str().ok_or("ELEMENT_NOT_FOUND")?;
                cleanup.handle = node["handle"].as_str().map(str::to_owned);
                self.command("input.performActions",json!({"context":context,"actions":[{"type":"pointer","id":format!("nodecloak-{}",g.id),"parameters":{"pointerType":"mouse"},"actions":[{"type":"pointerMove","x":0,"y":0,"origin":{"type":"element","element":{"sharedId":shared}}},{"type":"pointerDown","button":0},{"type":"pointerUp","button":0}]}]}),None,cancel)
            } else {
                let object = node["objectId"].as_str().ok_or("ELEMENT_NOT_FOUND")?;
                cleanup.handle = Some(object.into());
                (|| {
                    self.command(
                        "DOM.scrollIntoViewIfNeeded",
                        json!({"objectId":object}),
                        Some(&context),
                        cancel,
                    )?;
                    let model = self.command(
                        "DOM.getBoxModel",
                        json!({"objectId":object}),
                        Some(&context),
                        cancel,
                    )?;
                    let q = model["model"]["content"]
                        .as_array()
                        .filter(|q| q.len() == 8)
                        .ok_or("ELEMENT_NOT_VISIBLE")?;
                    let x = (0..4).map(|i| q[i * 2].as_f64().unwrap_or(0.)).sum::<f64>() / 4.;
                    let y = (0..4)
                        .map(|i| q[i * 2 + 1].as_f64().unwrap_or(0.))
                        .sum::<f64>()
                        / 4.;
                    self.command(
                        "Input.dispatchMouseEvent",
                        json!({"type":"mouseMoved","x":x,"y":y}),
                        Some(&context),
                        cancel,
                    )?;
                    cleanup.mouse = Some((x, y));
                    let down = self.command(
                        "Input.dispatchMouseEvent",
                        json!({"type":"mousePressed","x":x,"y":y,"button":"left","clickCount":1}),
                        Some(&context),
                        cancel,
                    )?;
                    if down["dialog_open"] == true {
                        return Ok(down);
                    }
                    cleanup.mouse = None;
                    self.command(
                        "Input.dispatchMouseEvent",
                        json!({"type":"mouseReleased","x":x,"y":y,"button":"left","clickCount":1}),
                        Some(&context),
                        &AtomicBool::new(false),
                    )
                })()
            };
            if result.as_ref().is_ok_and(|v| v["dialog_open"] == true) {
                self.automation_cleanup.push(cleanup);
            } else {
                let _ = self.finish_input(cleanup);
            }
            let result = result?;
            return Ok(if result["dialog_open"] == true {
                result
            } else {
                json!({"clicked":true})
            });
        }
        if tool == "page_upload" {
            let paths: Vec<String> = a["paths"]
                .as_array()
                .ok_or("PATHS_REQUIRED")?
                .iter()
                .map(|v| validate_file(g, v.as_str().unwrap_or("")).map(|p| browser_path(&p)))
                .collect::<Result<_, _>>()?;
            a["action"] = json!("file_node");
            let node = self.evaluate(&context, &frame, g, &a, true, cancel)?;
            let mut cleanup = Cleanup {
                context: context.clone(),
                realm: format!("nodecloak-{}", g.id),
                handle: None,
                mouse: None,
            };
            let result = if self.firefox {
                let shared = node["sharedId"].as_str().ok_or("ELEMENT_NOT_FILE_INPUT")?;
                cleanup.handle = node["handle"].as_str().map(str::to_owned);
                self.command(
                    "input.setFiles",
                    json!({"context":context,"element":{"sharedId":shared},"files":paths}),
                    None,
                    cancel,
                )
            } else {
                let object = node["objectId"].as_str().ok_or("ELEMENT_NOT_FILE_INPUT")?;
                cleanup.handle = Some(object.into());
                self.command(
                    "DOM.setFileInputFiles",
                    json!({"objectId":object,"files":paths}),
                    Some(&context),
                    cancel,
                )
            };
            if result.as_ref().is_ok_and(|v| v["dialog_open"] == true) {
                self.automation_cleanup.push(cleanup);
            } else {
                let _ = self.finish_input(cleanup);
            }
            let result = result?;
            return Ok(
                json!({"files_selected":paths.len(),"dialog_open":result["dialog_open"] == true}),
            );
        }
        if tool == "page_snapshot" {
            a["_new_snapshot_id"] = json!(uuid::Uuid::new_v4().to_string());
        }
        a["action"] = json!(match tool {
            "page_snapshot" => "snapshot",
            "page_click" => "click",
            "page_fill" => "fill",
            "page_select" => "select",
            "page_scroll" => "scroll",
            _ => return Err("UNKNOWN_TOOL".into()),
        });
        self.evaluate(&context, &frame, g, &a, false, cancel)
    }
}
// CDP can retire a page/OOPIF session between enumeration and inspection.
// Do not suppress other browser errors (or cancellation) as a disappearing target.
fn closed_target_error(error: &str) -> bool {
    matches!(error,
        "BROWSER_REJECTED: Session with given id not found."
        | "BROWSER_REJECTED: Target closed"
        | "BROWSER_REJECTED: Target closed.")
}
fn decode_remote(v: &Value) -> Result<Value, String> {
    Ok(match v["type"].as_str().unwrap_or("") {
        "object" => {
            let mut map = serde_json::Map::new();
            for pair in v["value"].as_array().into_iter().flatten() {
                let key = pair[0].as_str().ok_or("INVALID_BROWSER_RESULT")?;
                map.insert(key.into(), decode_remote(&pair[1])?);
            }
            Value::Object(map)
        }
        "array" => Value::Array(
            v["value"]
                .as_array()
                .ok_or("INVALID_BROWSER_RESULT")?
                .iter()
                .map(decode_remote)
                .collect::<Result<_, _>>()?,
        ),
        "undefined" | "null" => Value::Null,
        "string" | "number" | "boolean" => v["value"].clone(),
        _ => return Err("INVALID_BROWSER_RESULT".into()),
    })
}
fn validate_file(g: &Grant, path: &str) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(path).map_err(|_| "FILE_NOT_FOUND")?;
    let allowed = g
        .file_roots
        .iter()
        .any(|r| std::fs::canonicalize(r).is_ok_and(|root| path.starts_with(root)));
    if !allowed {
        return Err("FILE_NOT_AUTHORIZED".into());
    }
    let m = std::fs::metadata(&path).map_err(|_| "FILE_NOT_FOUND")?;
    if !m.is_file() || m.len() > 25_000_000 {
        return Err("FILE_TOO_LARGE_OR_INVALID".into());
    }
    Ok(path)
}
pub(super) fn download_dir(g: &Grant, profile: &str) -> Result<PathBuf, String> {
    let base = std::fs::canonicalize(
        g.file_roots
            .first()
            .ok_or("FILE_DIRECTORY_NOT_AUTHORIZED")?,
    )
    .map_err(|_| "FILE_DIRECTORY_NOT_FOUND")?;
    let id = profile.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "_");
    let dir = base.join(format!("nodecloak-{}-{id}", g.id));
    if dir.exists() {
        let canonical = std::fs::canonicalize(&dir).map_err(|_| "INVALID_DOWNLOAD_DIRECTORY")?;
        if !canonical.starts_with(&base) {
            return Err("INVALID_DOWNLOAD_DIRECTORY".into());
        }
    }
    std::fs::create_dir_all(&dir).map_err(|_| "DOWNLOAD_DIRECTORY_WRITE_FAILED")?;
    std::fs::canonicalize(dir).map_err(|_| "INVALID_DOWNLOAD_DIRECTORY".into())
}

pub(super) fn browser_path(path: &Path) -> String {
    let text = path.to_string_lossy().to_string();
    #[cfg(windows)]
    {
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", unc);
        }
        if let Some(local) = text.strip_prefix(r"\\?\") {
            return local.to_string();
        }
    }
    text
}

#[cfg(test)]
mod tree_tests {
    use super::*;
    use std::net::TcpListener;

    // Target.getTargets and Page.getFrameTree are separate protocol round trips:
    // closing a tab can invalidate either a page or OOPIF session between them.
    #[test]
    fn frame_tree_omits_closed_targets_but_keeps_live_tabs_and_real_errors() {
        for failure in ["Session with given id not found.", "Target closed", "Access denied"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut ws = tungstenite::accept(stream).unwrap();
                while let Ok(Message::Text(raw)) = ws.read() {
                    let request: Value = serde_json::from_str(&raw).unwrap();
                    let result = match request["method"].as_str().unwrap() {
                        "Target.getTargets" => json!({"targetInfos":[
                            {"targetId":"live","type":"page","url":"https://example.test/","title":"Live","attached":true},
                            {"targetId":"closing-page","type":"page","url":"https://closing.test/","title":"Closing","attached":true},
                            {"targetId":"closing-frame","type":"iframe","url":"https://closing.test/frame","title":"Closing frame","attached":true},
                            {"targetId":"live-frame","type":"iframe","url":"https://frame.test/","title":"Frame","attached":true}
                        ]}),
                        "Page.getFrameTree" if request["sessionId"] == "live-session" => json!({"frameTree":{"frame":{"id":"live","url":"https://example.test/"}}}),
                        "Page.getFrameTree" if request["sessionId"] == "live-frame-session" => json!({"frameTree":{"frame":{"id":"live-frame","parentId":"live","url":"https://frame.test/"}}}),
                        "Page.getFrameTree" => {
                            ws.send(Message::Text(json!({"id":request["id"],"sessionId":request["sessionId"],"error":{"code":-32001,"message":failure}}).to_string().into())).unwrap();
                            continue;
                        }
                        method => panic!("unexpected protocol call {method}"),
                    };
                    ws.send(Message::Text(json!({"id":request["id"],"result":result}).to_string().into())).unwrap();
                }
            });
            let (mut socket, _) = tungstenite::connect(format!("ws://{address}")).unwrap();
            if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
                stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
            }
            let mut controller = BrowserControl {
                diagnostic:None,plan: ControlPlan { profile_settings: false, automation: true },
                protocol: Protocol {worker_setups:HashMap::new(),document_setups:HashMap::new(),barrier_owner:None,socket,next:0,events:VecDeque::new(),last_error:None,navigation:None,reconnect_endpoint:None},
                firefox:false,region:regional::initial(&crate::profiles::Preferences::default()).unwrap(),
                permission:PermissionMode::Ask,primary:"live-session".into(),sessions:HashSet::new(),
                targets:HashMap::from([("live".into(),"live-session".into()),("closing-page".into(),"closing-page-session".into()),("closing-frame".into(),"closing-frame-session".into()),("live-frame".into(),"live-frame-session".into())]),
                workers:HashSet::new(),pages:HashSet::new(),advanced:Default::default(),fingerprint:None,
                fingerprint_sessions:HashSet::new(),internal_workers:HashSet::new(),session_checked:Instant::now(),session_capture_ready:false,normal_context:None,
                automation_fatal:None,automation_dialogs:HashMap::new(),automation_warnings:Vec::new(),automation_cleanup:Vec::new(),
                download_lease:None,download_subscribed:false,download_checked:Instant::now(),
            };
            let result = controller.tree(&AtomicBool::new(false));
            drop(controller);
            server.join().unwrap();
            if failure == "Access denied" {
                assert!(result.unwrap_err().contains("Access denied"));
            } else {
                let items = result.unwrap();
                assert_eq!(items.iter().map(|v| v["frame_id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["live","live-frame"]);
                assert_eq!(items[1]["tab_id"],"live");
            }
        }
    }
}
