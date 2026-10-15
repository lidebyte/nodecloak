//! Native namespace metadata, read only from closed, staged profiles.
use super::browser_session::{origin, Cookie, Partition, SiteKey, PARTITIONED_STORAGE_ERROR};
use crate::profiles;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::{collections::BTreeSet, path::Path};

pub(super) fn sqlite(path: &Path) -> Result<Connection, String> {
    profiles::guard_tree(path)?;
    if std::fs::metadata(path)
        .map_err(|_| "无法读取浏览器存储索引")?
        .len()
        > 128 << 20
    {
        return Err("浏览器存储索引过大".into());
    }
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "无法读取浏览器存储索引")?;
    c.busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "无法读取浏览器存储索引")?;
    Ok(c)
}
pub(super) fn chromium_key(value: &str) -> Result<SiteKey, String> {
    if value.len() > 4096 {
        return Err("网站存储键过长".into());
    }
    let (base, attribute) = value
        .split_once('^')
        .map(|(a, b)| (a, Some(b)))
        .unwrap_or((value, None));
    let o = origin(base)?;
    let partition_key = match attribute {
        None => None,
        Some(a) if a.starts_with('0') && !a.contains('^') => Some(Partition {
            top_level_site: origin(&a[1..])?,
            has_cross_site_ancestor: true,
        }),
        Some("31") => Some(Partition {
            top_level_site: o.clone(),
            has_cross_site_ancestor: true,
        }),
        _ => return Err(PARTITIONED_STORAGE_ERROR.into()),
    };
    Ok(SiteKey {
        origin: o,
        partition_key,
    })
}
// Internal browser pages own these partitions; they are not portable website
// sessions. Filter only during local discovery, keeping imported data web-only.
pub(super) fn chromium_web_key(value: &str) -> Result<Option<SiteKey>, String> {
    if value.len() > 4096 {
        return Err("网站存储键过长".into());
    }
    if let Some((base, attribute)) = value.split_once('^') {
        origin(base)?;
        if !attribute.contains('^') {
            if let Some(top) = attribute.strip_prefix('0')
                .and_then(|top| reqwest::Url::parse(top).ok()) {
                if matches!(top.scheme(), "edge" | "chrome" | "chrome-extension" | "devtools")
                    && top.host_str().is_some()
                    && top.username().is_empty() && top.password().is_none()
                    && matches!(top.path(), "" | "/")
                    && top.query().is_none() && top.fragment().is_none() {
                    return Ok(None);
                }
            }
        }
    }
    chromium_key(value).map(Some)
}
pub(super) fn firefox_partition(attributes: &str) -> Result<Option<Partition>, String> {
    let attributes = attributes.strip_prefix('^').unwrap_or(attributes);
    if attributes.is_empty() {
        return Ok(None);
    }
    let fields = reqwest::Url::parse(&format!("https://namespace.invalid/?{attributes}"))
        .map_err(|_| "网站存储属性无效")?
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect::<Vec<_>>();
    if fields.len() != 1 || fields[0].0 != "partitionKey" {
        return Err(PARTITIONED_STORAGE_ERROR.into());
    }
    let raw = &fields[0].1;
    let parts = raw
        .strip_prefix('(')
        .and_then(|v| v.strip_suffix(')'))
        .ok_or(PARTITIONED_STORAGE_ERROR)?
        .split(',')
        .collect::<Vec<_>>();
    if !(2..=4).contains(&parts.len()) || !matches!(parts[0], "http" | "https") {
        return Err(PARTITIONED_STORAGE_ERROR.into());
    }
    let foreign = parts.last() == Some(&"f");
    let port = if parts.len() > 2 && parts[2] != "f" {
        let n = parts[2]
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or("网站分区端口无效")?;
        format!(":{n}")
    } else {
        String::new()
    };
    if parts.len() == 4 && (!foreign || parts[2] == "f") {
        return Err(PARTITIONED_STORAGE_ERROR.into());
    }
    Ok(Some(Partition {
        top_level_site: origin(&format!("{}://{}{port}", parts[0], parts[1]))?,
        has_cross_site_ancestor: foreign,
    }))
}
pub(super) fn firefox_key(value: &str) -> Result<SiteKey, String> {
    let (base, attributes) = value.split_once('^').unwrap_or((value, ""));
    let o = origin(&base.replacen("+++", "://", 1).replace('+', ":"))?;
    let mut partition_key = firefox_partition(attributes)?;
    if let Some(p) = &mut partition_key {
        // Firefox serializes "f" only for a same-site document behind a
        // cross-site ancestor. An ordinary cross-site iframe omits that bit.
        if !same_site(&o, &p.top_level_site) {
            if p.has_cross_site_ancestor {
                return Err(PARTITIONED_STORAGE_ERROR.into());
            }
            p.has_cross_site_ancestor = true;
        } else if !p.has_cross_site_ancestor {
            return Err(PARTITIONED_STORAGE_ERROR.into());
        }
    }
    Ok(SiteKey {
        origin: o,
        partition_key,
    })
}
pub(super) fn same_site(value: &str, top: &str) -> bool {
    let (Ok(a), Ok(b)) = (reqwest::Url::parse(value), reqwest::Url::parse(top)) else {
        return false;
    };
    a.scheme() == b.scheme()
        && (a.host_str() == b.host_str()
            || a.host_str()
                .is_some_and(|h| h.ends_with(&format!(".{}", b.host_str().unwrap_or("")))))
}
pub(super) fn firefox_attributes(p: &Partition) -> Result<String, String> {
    let parsed = reqwest::Url::parse(&p.top_level_site).map_err(|_| "Cookie 分区无效")?;
    let host = parsed.host_str().ok_or("Cookie 分区无效")?;
    let raw = format!(
        "({},{host}{}{})",
        parsed.scheme(),
        parsed.port().map(|p| format!(",{p}")).unwrap_or_default(),
        if p.has_cross_site_ancestor { ",f" } else { "" }
    );
    let mut url = reqwest::Url::parse("https://namespace.invalid/").unwrap();
    url.query_pairs_mut().append_pair("partitionKey", &raw);
    Ok(format!("^{}", url.query().unwrap()))
}
pub(super) fn quota_keys(path: &Path, out: &mut BTreeSet<SiteKey>) -> Result<(), String> {
    for relative in ["Default/QuotaManager", "Default/WebStorage/QuotaManager"] {
        let db = path.join(relative);
        if !db.exists() {
            continue;
        }
        let c = sqlite(&db)?;
        let table: Option<String> = c
            .query_row(
                "SELECT type FROM sqlite_schema WHERE name='buckets'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "浏览器存储索引无效")?;
        if table.is_none() {
            continue;
        }
        if table.as_deref() != Some("table") {
            return Err("浏览器存储索引无效".into());
        }
        let mut stmt = c
            .prepare("SELECT storage_key,name FROM buckets LIMIT 100001")
            .map_err(|_| "浏览器存储索引格式不支持")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|_| "浏览器存储索引无效")?;
        for (index, row) in rows.enumerate() {
            if index >= 100000 {
                return Err("网站存储记录过多".into());
            }
            let (key, name) = row.map_err(|_| "浏览器存储索引无效")?;
            if !key.starts_with("http://") && !key.starts_with("https://") {
                continue;
            }
            let Some(key) = chromium_web_key(&key)? else { continue };
            if name != "_default" {
                return Err("此副本包含命名网站存储桶，保留本机数据".into());
            }
            out.insert(key);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_partition_filter_preserves_strict_website_validation() {
        for value in [
            "https://login.example/path/^0edge://copilot-ntp",
            "https://user:password@login.example/^0edge://copilot-ntp",
            "https://login.example/^0edge://user:password@copilot-ntp",
            "https://login.example/^0edge://copilot-ntp/path",
            "https://login.example/^0edge://copilot-ntp?query=value",
            "https://login.example/^0edge://copilot-ntp#fragment",
            "https://login.example/^0edge://copilot-ntp^31",
            "https://login.example/^0file:///tmp/profile",
            "https://login.example/^0about:blank",
            "https://login.example/^4opaque",
        ] {
            assert!(chromium_web_key(value).is_err(), "{value}");
        }
        assert!(chromium_web_key("https://login.example/^31").unwrap().is_some());
        assert!(chromium_web_key(&format!("https://login.example/^0edge://{}", "a".repeat(4096))).is_err());
    }
    #[test]
    fn nonportable_namespaces_are_rejected_without_flattening() {
        for key in [
            "https://login.example/^11^22",
            "https://login.example/^4opaque",
            "https://login.example/^0https://shop.example^31",
        ] {
            assert!(chromium_key(key).is_err(), "{key}");
        }
        for key in [
            "https+++login.example^userContextId=2",
            "https+++login.example^partitionKey=%28https%2Clogin.example%29",
            "https+++login.example^partitionKey=%28https%2Cshop.example%2Cf%29",
            "https+++login.example^partitionKey=%28https%2Cshop.example%29&userContextId=2",
        ] {
            assert!(firefox_key(key).is_err(), "{key}");
        }
        let same = chromium_key("https://login.example/^31").unwrap();
        assert_eq!(
            same.partition_key.as_ref().unwrap().top_level_site,
            "https://login.example"
        );
        assert!(same.partition_key.unwrap().has_cross_site_ancestor);
    }
    #[test]
    fn firefox_partition_requires_exact_native_tuple_layout() {
        for raw in [
            "(https,example.com)",
            "(https,example.com,f)",
            "(https,example.com,8443)",
            "(https,example.com,8443,f)",
        ] {
            assert!(
                firefox_partition(&format!("partitionKey={raw}")).is_ok(),
                "{raw}"
            );
        }
        for raw in [
            "(https,example.com,f,f)",
            "(https,example.com,)",
            "(https,example.com,8443,x)",
            "(https,example.com,0)",
        ] {
            assert!(
                firefox_partition(&format!("partitionKey={raw}")).is_err(),
                "{raw}"
            );
        }
    }
    #[test]
    fn quota_discovery_preserves_idb_only_partitions_and_ignores_extension_buckets() {
        for location in ["Default/QuotaManager", "Default/WebStorage/QuotaManager"] {
            let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
            std::fs::create_dir_all(dir.path().join(location).parent().unwrap()).unwrap();
            let c = Connection::open(dir.path().join(location)).unwrap();
            c.execute_batch("CREATE TABLE buckets(storage_key TEXT,name TEXT); INSERT INTO buckets VALUES ('https://login.example/^0https://shop.example','_default'),('chrome-extension://extension/','private');").unwrap();
            drop(c);
            let mut keys = BTreeSet::new();
            quota_keys(dir.path(), &mut keys).unwrap();
            assert_eq!(keys.len(), 1);
            assert_eq!(
                keys.first()
                    .unwrap()
                    .partition_key
                    .as_ref()
                    .unwrap()
                    .top_level_site,
                "https://shop.example"
            );
        }
    }
    #[test]
    fn firefox_cookie_database_versions_preserve_expiry_and_tcp_attribute() {
        for (version, units) in [(15, 1), (17, 1000)] {
            let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
            let c = Connection::open(dir.path().join("cookies.sqlite")).unwrap();
            c.execute_batch(&format!("PRAGMA user_version={version}; CREATE TABLE moz_cookies(name TEXT,value TEXT,host TEXT,path TEXT,isSecure INTEGER,isHttpOnly INTEGER,sameSite INTEGER,expiry INTEGER,originAttributes TEXT,isPartitionedAttributeSet INTEGER);")).unwrap();
            let expiry = chrono::Utc::now().timestamp() + 604800;
            c.execute("INSERT INTO moz_cookies VALUES ('session','synthetic','login.example','/',1,1,256,?,'^partitionKey=%28https%2Cshop.example%29',0)",[expiry*units]).unwrap();
            drop(c);
            let cookies = firefox_cookies(dir.path()).unwrap();
            assert_eq!(cookies.len(), 1);
            assert_eq!(cookies[0].expires, expiry as f64);
            assert_eq!(cookies[0].same_site, "Default");
            assert!(!cookies[0].partitioned_attribute);
            assert_eq!(
                cookies[0].firefox_partition_key.as_deref(),
                Some("^partitionKey=%28https%2Cshop.example%29")
            );
            let c = Connection::open(dir.path().join("cookies.sqlite")).unwrap();
            c.execute(
                "UPDATE moz_cookies SET originAttributes='^userContextId=2'",
                [],
            )
            .unwrap();
            drop(c);
            assert!(write_firefox_partitioned_cookies(dir.path(), &[]).is_err());
            let c = Connection::open(dir.path().join("cookies.sqlite")).unwrap();
            assert_eq!(
                c.query_row("SELECT value FROM moz_cookies", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "synthetic"
            );
        }
    }
}
pub(super) fn firefox_cookies(path: &Path) -> Result<Vec<Cookie>, String> {
    let db = path.join("cookies.sqlite");
    if !db.exists() {
        return Ok(Vec::new());
    }
    let c = sqlite(&db)?;
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|_| "Firefox Cookie 版本无效")?;
    if !(13..=17).contains(&version) {
        return Err("Firefox Cookie 数据版本不支持，保留本机数据".into());
    }
    let expiry_units = if version >= 16 { 1000 } else { 1 };
    let mut stmt=c.prepare("SELECT name,value,host,path,isSecure,isHttpOnly,sameSite,expiry,originAttributes,isPartitionedAttributeSet FROM moz_cookies WHERE expiry>? LIMIT 10001").map_err(|_|"Firefox Cookie 格式不支持")?;
    let rows = stmt
        .query_map([chrono::Utc::now().timestamp() * expiry_units], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, bool>(9)?,
            ))
        })
        .map_err(|_| "无法读取 Firefox Cookie")?;
    let mut out = Vec::new();
    for row in rows {
        if out.len() >= 10000 {
            return Err("Cookie 数量超过限制".into());
        }
        let (name, value, domain, path, secure, http_only, same, expiry, attrs, explicit) =
            row.map_err(|_| "Firefox Cookie 数据无效")?;
        let mut partition_key = firefox_partition(&attrs)?;
        let firefox_partition_key = partition_key.as_ref().map(|_| attrs.clone());
        if let Some(p) = &mut partition_key {
            if !same_site(
                &format!(
                    "{}://{}",
                    reqwest::Url::parse(&p.top_level_site).unwrap().scheme(),
                    domain.trim_start_matches('.')
                ),
                &p.top_level_site,
            ) {
                p.has_cross_site_ancestor = true;
            }
        }
        out.push(Cookie {
            name,
            value,
            domain,
            path,
            secure,
            http_only,
            same_site: match same {
                0 => "None",
                1 => "Lax",
                2 => "Strict",
                256 => "Default",
                _ => return Err("Firefox Cookie SameSite 无效".into()),
            }
            .into(),
            expires: expiry as f64 / expiry_units as f64,
            partitioned_attribute: partition_key.is_none() || explicit,
            partition_key,
            firefox_partition_key,
        });
    }
    Ok(out)
}
pub(super) fn write_firefox_partitioned_cookies(
    path: &Path,
    cookies: &[Cookie],
) -> Result<(), String> {
    crate::platform::ensure_profile_closed(path)?;
    let db = path.join("cookies.sqlite");
    profiles::guard_tree(&db)?;
    let mut c = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "无法写入 Firefox Cookie")?;
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|_| "Firefox Cookie 版本无效")?;
    if !(13..=17).contains(&version) {
        return Err("Firefox Cookie 数据版本不支持，保留本机数据".into());
    }
    let expiry_units = if version >= 16 { 1000 } else { 1 };
    let mut stmt = c
        .prepare("PRAGMA table_info(moz_cookies)")
        .map_err(|_| "Firefox Cookie 格式不支持")?;
    let columns = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|_| "Firefox Cookie 格式不支持")?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|_| "Firefox Cookie 格式不支持")?;
    drop(stmt);
    if cookies.iter().any(|c| c.partition_key.is_some())
        && !columns.contains("isPartitionedAttributeSet")
    {
        return Err("请更新 Firefox 后再迁移分区 Cookie".into());
    }
    let tx = c.transaction().map_err(|_| "无法写入 Firefox Cookie")?;
    {
        let mut stmt = tx
            .prepare("SELECT DISTINCT originAttributes FROM moz_cookies WHERE originAttributes<>''")
            .map_err(|_| "Firefox Cookie 格式不支持")?;
        for attrs in stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| "Firefox Cookie 数据无效")?
        {
            firefox_partition(&attrs.map_err(|_| "Firefox Cookie 数据无效")?)?;
        }
    }
    tx.execute("DELETE FROM moz_cookies WHERE originAttributes<>''", [])
        .map_err(|_| "无法清理旧的 Firefox Cookie 分区")?;
    use rusqlite::types::Value as Sql;
    for cookie in cookies
        .iter()
        .filter(|c| c.expires < 0. || c.expires > chrono::Utc::now().timestamp() as f64)
    {
        if cookie.partition_key.is_none() {
            if cookie.expires > 0. {
                // BiDi truncates to integer seconds. Keep the original native
                // precision while leaving its validated unpartitioned row intact.
                tx.execute("UPDATE moz_cookies SET expiry=? WHERE originAttributes='' AND name=? AND host=? AND path=?",rusqlite::params![(cookie.expires*expiry_units as f64).floor() as i64,cookie.name,cookie.domain,cookie.path]).map_err(|_| "无法保留 Firefox Cookie 有效期")?;
            }
            continue;
        }
        if cookie.expires < 0. {
            return Err("Firefox 浏览器退出时 Cookie 无法持久迁移".into());
        }
        let same = match cookie.same_site.as_str() {
            "None" => 0,
            "Lax" => 1,
            "Strict" => 2,
            "Default" => 256,
            _ => return Err("Cookie SameSite 无效".into()),
        };
        let now = chrono::Utc::now().timestamp_micros();
        let attrs = if let Some(native) = &cookie.firefox_partition_key {
            native.clone()
        } else {
            let mut p = cookie.partition_key.as_ref().unwrap().clone();
            if !same_site(
                &format!(
                    "{}://{}",
                    reqwest::Url::parse(&p.top_level_site).unwrap().scheme(),
                    cookie.domain.trim_start_matches('.')
                ),
                &p.top_level_site,
            ) {
                p.has_cross_site_ancestor = false;
            }
            firefox_attributes(&p)?
        };
        let mut fields = vec![
            ("originAttributes", Sql::Text(attrs)),
            ("name", Sql::Text(cookie.name.clone())),
            ("value", Sql::Text(cookie.value.clone())),
            ("host", Sql::Text(cookie.domain.clone())),
            ("path", Sql::Text(cookie.path.clone())),
            (
                "expiry",
                Sql::Integer((cookie.expires * expiry_units as f64).floor() as i64),
            ),
            ("lastAccessed", Sql::Integer(now)),
            ("creationTime", Sql::Integer(now)),
            ("isSecure", Sql::Integer(cookie.secure as i64)),
            ("isHttpOnly", Sql::Integer(cookie.http_only as i64)),
            ("sameSite", Sql::Integer(same)),
        ];
        for (key, value) in [
            ("rawSameSite", same),
            ("schemeMap", if cookie.secure { 2 } else { 0 }),
            (
                "isPartitionedAttributeSet",
                cookie.partitioned_attribute as i64,
            ),
            ("inBrowserElement", 0),
            ("updateTime", now),
        ] {
            if columns.contains(key) {
                fields.push((key, Sql::Integer(value)));
            }
        }
        let sql = format!(
            "INSERT OR REPLACE INTO moz_cookies ({}) VALUES ({})",
            fields.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(","),
            vec!["?"; fields.len()].join(",")
        );
        tx.execute(
            &sql,
            rusqlite::params_from_iter(fields.iter().map(|(_, v)| v)),
        )
        .map_err(|_| "无法写入 Firefox Cookie 分区")?;
    }
    tx.commit().map_err(|_| "无法保存 Firefox Cookie 分区")?;
    Ok(())
}
