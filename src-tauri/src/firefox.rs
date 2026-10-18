//! Preferences are parsed as data, never evaluated as JavaScript.
use serde_json::{json, Value};
use std::{fs, path::Path};

pub const FONT_LIST: &str = "Arial,Calibri,Cambria,Consolas,Courier,Courier New,Georgia,Helvetica,Helvetica Neue,Menlo,Monaco,Segoe UI,Segoe UI Emoji,Segoe UI Symbol,Tahoma,Times,Times New Roman,Trebuchet MS,Verdana,Apple Color Emoji";
pub fn fields(id: &str) -> Result<Vec<(&'static str, Value)>, String> {
    Ok(match id {
        "fonts" => vec![("font.system.whitelist", json!(FONT_LIST))],
        "language" => vec![
            ("intl.accept_languages", json!("en-US,en")),
            ("intl.locale.requested", json!("en-US")),
            ("intl.regional_prefs.use_os_locales", json!(false)),
        ],
        "webrtc" => vec![("media.peerconnection.enabled", json!(false))],
        "dns" => vec![
            ("network.trr.mode", json!(3)),
            (
                "network.trr.uri",
                json!("https://cloudflare-dns.com/dns-query"),
            ),
        ],
        "tracking" => vec![
            ("privacy.globalprivacycontrol.enabled", json!(true)),
            (
                "privacy.globalprivacycontrol.functionality.enabled",
                json!(true),
            ),
        ],
        "startup" => vec![
            ("browser.startup.homepage", json!("about:blank")),
            ("browser.startup.page", json!(0)),
            ("browser.startup.homepage_override.mstone", json!("ignore")),
        ],
        _ => return Err("不支持的 Firefox 设置".into()),
    })
}
fn allowed(key: &str) -> bool {
    [
        "devtools.debugger.remote-enabled",
        "devtools.chrome.enabled",
        "devtools.debugger.prompt-connection",
        "devtools.debugger.force-local",
        "network.proxy.type",
        "network.proxy.http",
        "network.proxy.http_port",
        "network.proxy.ssl",
        "network.proxy.ssl_port",
        "network.proxy.socks",
        "network.proxy.socks_port",
        "network.proxy.no_proxies_on",
        "network.proxy.socks_remote_dns",
        "network.proxy.failover_direct",
        "permissions.default.desktop-notification",
        "permissions.default.geo",
        "geo.enabled",
        "geo.provider.network.url",
        "geo.provider.testing",
        "geo.provider.network.scan",
        "geo.provider.network.timeToWaitBeforeSending",
        "permissions.default.camera",
        "permissions.default.microphone",
        "permissions.default.image",
        "webgl.disabled",
        "privacy.resistFingerprinting",
        "privacy.donottrackheader.enabled",
        "privacy.fingerprintingProtection",
        "privacy.fingerprintingProtection.pbmode",
        "privacy.baselineFingerprintingProtection.overrides",
        "dom.webgpu.enabled",
        "media.webspeech.synth.enabled",
        "dom.battery.enabled",
        "dom.maxHardwareConcurrency",
        "media.peerconnection.ice.default_address_only",
        "media.peerconnection.ice.no_host",
        "media.peerconnection.ice.proxy_only",
        "remote.prefs.recommended",
        "marionette.port",
        "termsofuse.bypassNotification",
        "dom.file.createInChild",
    ]
    .contains(&key)
        || ["fonts", "language", "webrtc", "dns", "tracking", "startup"]
            .iter()
            .any(|id| fields(id).unwrap().iter().any(|(k, _)| *k == key))
}

pub fn prepare_startup(profile: &Path) -> Result<(), String> {
    let changes = fields("startup")?
        .into_iter()
        .map(|(key, value)| {
            Ok((
                key.to_owned(),
                snapshot(profile, key)?,
                json!({"user":value,"runtime":value}),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    apply(profile, &changes, false)
}
fn document(path: &Path) -> Result<String, String> {
    match fs::metadata(path) {
        Ok(m) if m.len() > 16 * 1024 * 1024 => Err("Firefox 配置超出大小限制".into()),
        Ok(_) => fs::read_to_string(path).map_err(|e| format!("无法读取 Firefox 配置：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}
fn value_in(text: &str, key: &str) -> Result<Option<Value>, String> {
    let mut found = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || !trimmed.contains(key) {
            continue;
        }
        // Ambiguous/multiline controlled preferences must be reviewed instead of overwritten.
        let body = trimmed
            .strip_prefix("user_pref(")
            .and_then(|s| s.strip_suffix(");"))
            .ok_or("Firefox 设置格式无法安全解析，原文件已保留")?;
        let (name, value) = body.split_once(',').ok_or("Firefox 设置格式无效")?;
        let name: String = serde_json::from_str(name.trim()).map_err(|_| "Firefox 设置名称无效")?;
        if name != key {
            continue;
        }
        if found.is_some() {
            return Err("Firefox 配置含重复设置，原文件已保留".into());
        }
        let v: Value =
            serde_json::from_str(value.trim()).map_err(|_| "Firefox 设置值无法安全解析")?;
        if !(v.is_boolean() || v.is_string() || v.as_i64().is_some()) {
            return Err("Firefox 设置类型无效".into());
        }
        found = Some(v);
    }
    Ok(found)
}
pub fn snapshot(profile: &Path, key: &str) -> Result<Value, String> {
    if !allowed(key) {
        return Err("Firefox 设置不在允许列表内".into());
    }
    Ok(
        json!({"user":value_in(&document(&profile.join("user.js"))?,key)?, "runtime":value_in(&document(&profile.join("prefs.js"))?,key)?}),
    )
}
const GEO_FIELDS: [&str; 4] = [
    "geo.provider.network.url",
    "geo.provider.testing",
    "geo.provider.network.scan",
    "geo.provider.network.timeToWaitBeforeSending",
];
/// Reserve originals before native runtime configuration for an IP-selected
/// position. Do not publish placeholder coordinates while lookup is pending.
pub(crate) fn reserve_geolocation(profile: &Path) -> Result<(), String> {
    crate::platform::ensure_profile_closed(profile)?;
    let file = profile.join("nodecloak-geolocation-original.json");
    crate::profiles::guard_tree(&file)?;
    if file.exists() { return Err("Firefox 原定位配置尚未恢复，请重试启动".into()); }
    let values = GEO_FIELDS.into_iter().map(|key| snapshot(profile,key).map(|v|(key.to_owned(),v))).collect::<Result<Vec<_>,_>>()?;
    crate::engine::atomic_write(&file,&serde_json::to_vec(&values).map_err(|_|"定位配置备份无效")?)
}

/// user.js is owned by this application and only read by Firefox at startup.
/// Never rewrite Firefox-owned prefs.js while a browser is running.
pub(crate) fn restore_user_preferences(profile: &Path, changes: &[(String, Value, Value)]) -> Result<(), String> {
    let file = profile.join("user.js");
    let mut text = document(&file)?;
    for (key,before,after) in changes {
        if !allowed(key) || before.get("user").is_none() || after.get("user").is_none()
            || value_in(&text,key)? != (!after["user"].is_null()).then(||after["user"].clone()) {
            return Err("Firefox 启动配置发生变化，原文件已保留".into());
        }
        text = replace(&text,key,&before["user"]);
    }
    crate::engine::atomic_write(&file,text.as_bytes())
}
/// Replace only Firefox's session-random PNG chunk while stable mode is on.
/// Preserve all unrelated baseline protections and restore both preference files.
pub fn prepare_stable_canvas(profile: &Path, enabled: bool) -> Result<(), String> {
    crate::platform::ensure_profile_closed(profile)?;
    let key = "privacy.baselineFingerprintingProtection.overrides";
    let backup = profile.join("nodecloak-fingerprint-original.json");
    let original = if backup.exists() {
        crate::profiles::guard_tree(&backup)?;
        if fs::metadata(&backup).map_err(|e| e.to_string())?.len() > 16384 {
            return Err("Firefox 指纹设置备份过大".into());
        }
        let value: Value = serde_json::from_slice(&fs::read(&backup).map_err(|e| e.to_string())?)
            .map_err(|_| "Firefox 指纹设置备份无效")?;
        if value
            .as_object()
            .is_none_or(|v| v.len() != 2 || !v.contains_key("user") || !v.contains_key("runtime"))
            || ["user", "runtime"]
                .iter()
                .any(|field| !value[field].is_null() && !value[field].is_string())
        {
            return Err("Firefox 指纹设置备份无效".into());
        }
        Some(value)
    } else {
        None
    };
    if !enabled && original.is_none() {
        return Ok(());
    }
    let current = snapshot(profile, key)?;
    let desired = if enabled {
        let base = original.as_ref().unwrap_or(&current);
        let mut result = json!({});
        for field in ["user", "runtime"] {
            if !base[field].is_null() && !base[field].is_string() {
                return Err("Firefox 指纹覆盖设置类型无效".into());
            }
            let mut tokens = base[field]
                .as_str()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|token| {
                    !token.is_empty()
                        && token.trim_start_matches(['+', '-']) != "EfficientCanvasRandomization"
                })
                .collect::<Vec<_>>();
            tokens.push("-EfficientCanvasRandomization");
            result[field] = json!(tokens.join(","));
        }
        if original.is_none() {
            crate::engine::atomic_write(
                &backup,
                &serde_json::to_vec(&current).map_err(|e| e.to_string())?,
            )?;
        }
        result
    } else {
        original.unwrap()
    };
    apply(profile, &[(key.into(), current, desired)], false)?;
    if !enabled {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// Firefox's BiDi override is local to the top document process. Its native
/// provider also reaches out-of-process frames, using an inline response so no
/// device position or Wi-Fi data is sent to a network service.
pub fn prepare_geolocation(
    profile: &Path,
    region: Option<&crate::regional::ResolvedRegion>,
) -> Result<(), String> {
    use base64::Engine;
    crate::platform::ensure_profile_closed(profile)?;
    let backup = profile.join("nodecloak-geolocation-original.json");
    let original: Option<Vec<(String, Value)>> = if backup.exists() {
        crate::profiles::guard_tree(&backup)?;
        if fs::metadata(&backup).map_err(|e| e.to_string())?.len() > 16384 {
            return Err("Firefox 定位设置备份过大".into());
        }
        let values: Vec<(String, Value)> =
            serde_json::from_slice(&fs::read(&backup).map_err(|e| e.to_string())?)
                .map_err(|_| "Firefox 定位设置备份无效")?;
        if values.len() != GEO_FIELDS.len()
            || values
                .iter()
                .zip(GEO_FIELDS)
                .any(|((key, _), expected)| key != expected)
        {
            return Err("Firefox 定位设置备份字段无效".into());
        }
        Some(values)
    } else {
        None
    };
    let coordinates = region.and_then(|r| {
        r.latitude
            .zip(r.longitude)
            .map(|(lat, lng)| (lat, lng, r.accuracy))
    });
    if coordinates.is_none() && original.is_none() {
        return Ok(());
    }
    let current = GEO_FIELDS
        .into_iter()
        .map(|key| Ok((key.to_owned(), snapshot(profile, key)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let desired = if let Some((lat, lng, accuracy)) = coordinates {
        if !lat.is_finite()
            || !lng.is_finite()
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lng)
            || !(1.0..=100_000.0).contains(&accuracy)
        {
            return Err("Firefox 定位坐标无效".into());
        }
        if original.is_none() {
            crate::engine::atomic_write(
                &backup,
                &serde_json::to_vec(&current).map_err(|e| e.to_string())?,
            )?;
        }
        let uri = format!(
            "data:application/json;base64,{}",
            base64::engine::general_purpose::STANDARD
                .encode(json!({"location":{"lat":lat,"lng":lng},"accuracy":accuracy}).to_string())
        );
        [json!(uri), json!(true), json!(false), json!(100)]
            .into_iter()
            .map(|value| json!({"user":value,"runtime":value}))
            .collect::<Vec<_>>()
    } else {
        original
            .unwrap()
            .into_iter()
            .map(|(_, value)| value)
            .collect()
    };
    let changes = current
        .into_iter()
        .zip(desired)
        .map(|((key, before), after)| (key, before, after))
        .collect::<Vec<_>>();
    apply(profile, &changes, false)?;
    if coordinates.is_none() {
        fs::remove_file(backup).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub fn configured(profile: &Path, id: &str) -> Result<bool, String> {
    for (key, expected) in fields(id)? {
        let s = snapshot(profile, key)?;
        // Firefox may omit a preference equal to its default from prefs.js.
        // user.js still requests it on every startup; this is configuration, not web verification.
        if s["user"] != expected || (!s["runtime"].is_null() && s["runtime"] != expected) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn replace(text: &str, key: &str, value: &Value) -> String {
    let mut result = String::new();
    for line in text.split_inclusive('\n') {
        if value_in(line, key).ok().flatten().is_some() {
            continue;
        }
        result.push_str(line);
    }
    if !value.is_null() {
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&format!("user_pref({}, {});\n", json!(key), value));
    }
    result
}
pub fn apply(profile: &Path, changes: &[(String, Value, Value)], undo: bool) -> Result<(), String> {
    crate::platform::ensure_profile_closed(profile)?;
    let mut documents = [
        document(&profile.join("user.js"))?,
        document(&profile.join("prefs.js"))?,
    ];
    for (key, before, after) in changes {
        if !allowed(key) {
            return Err("Firefox 备份字段无效".into());
        }
        for (i, part) in ["user", "runtime"].iter().enumerate() {
            let original = before.get(part).ok_or("Firefox 备份缺失")?;
            let target = after.get(part).ok_or("Firefox 备份缺失")?;
            if ![original, target]
                .iter()
                .all(|v| v.is_null() || v.is_boolean() || v.is_string() || v.as_i64().is_some())
            {
                return Err("Firefox 备份类型无效".into());
            }
            let current = value_in(&documents[i], key)?.unwrap_or(Value::Null);
            let omitted_runtime = *part == "runtime"
                && current.is_null()
                && value_in(&documents[0], key)?.unwrap_or(Value::Null) == after["user"];
            if current != *target && current != *original && !omitted_runtime {
                return Err("Firefox 设置已被其他程序修改，本次未覆盖。".into());
            }
        }
    }
    for (key, before, after) in changes {
        let desired = if undo { before } else { after };
        for (i, part) in ["user", "runtime"].iter().enumerate() {
            documents[i] = replace(&documents[i], key, &desired[part]);
        }
    }
    for (i, name) in ["user.js", "prefs.js"].iter().enumerate() {
        crate::engine::atomic_write(&profile.join(name), documents[i].as_bytes())?;
    }
    for (key, before, after) in changes {
        if snapshot(profile, key)? != *if undo { before } else { after } {
            return Err("Firefox 设置写入后验证失败，可从记录恢复".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_canvas_preserves_other_baseline_controls_and_restores_originals() {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let profile = temp.path();
        let key = "privacy.baselineFingerprintingProtection.overrides";
        let user = format!("user_pref(\"{key}\", \"+ScreenAvailToResolution\");\n");
        let runtime = format!("user_pref(\"{key}\", \"-MaxTouchPointsCollapse\");\n");
        std::fs::write(profile.join("user.js"), &user).unwrap();
        std::fs::write(profile.join("prefs.js"), &runtime).unwrap();
        super::prepare_stable_canvas(profile, true).unwrap();
        let protected = super::snapshot(profile, key).unwrap();
        assert_eq!(
            protected["user"],
            "+ScreenAvailToResolution,-EfficientCanvasRandomization"
        );
        assert_eq!(
            protected["runtime"],
            "-MaxTouchPointsCollapse,-EfficientCanvasRandomization"
        );
        super::prepare_stable_canvas(profile, true).unwrap();
        assert_eq!(super::snapshot(profile, key).unwrap(), protected);
        super::prepare_stable_canvas(profile, false).unwrap();
        assert_eq!(
            super::snapshot(profile, key).unwrap(),
            serde_json::json!({"user":"+ScreenAvailToResolution","runtime":"-MaxTouchPointsCollapse"})
        );
        assert!(!profile.join("nodecloak-fingerprint-original.json").exists());
    }
    use super::*;
    #[test]
    fn native_geolocation_provider_restores_original_settings_and_preserves_login_data() {
        use base64::Engine;
        let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = dir.path();
        fs::write(p.join("user.js"),"user_pref(\"geo.provider.network.url\", \"https://original.example/location\");\nuser_pref(\"unrelated\", 42);\n").unwrap();
        fs::write(p.join("cookies.sqlite"), "login data").unwrap();
        let originals = GEO_FIELDS
            .into_iter()
            .map(|key| snapshot(p, key).unwrap())
            .collect::<Vec<_>>();
        let mut region =
            crate::regional::initial(&crate::profiles::Preferences::default()).unwrap();
        region.latitude = Some(1.3521);
        region.longitude = Some(103.8198);
        prepare_geolocation(p, Some(&region)).unwrap();
        region.latitude = Some(35.68);
        region.longitude = Some(139.69);
        prepare_geolocation(p, Some(&region)).unwrap();
        let snapshot = snapshot(p, GEO_FIELDS[0]).unwrap();
        let uri = snapshot["user"].as_str().unwrap();
        let payload: Value = serde_json::from_slice(
            &base64::engine::general_purpose::STANDARD
                .decode(uri.strip_prefix("data:application/json;base64,").unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["location"]["lat"], 35.68);
        assert_eq!(payload["location"]["lng"], 139.69);
        prepare_geolocation(p, None).unwrap();
        prepare_geolocation(p, None).unwrap();
        for (key, original) in GEO_FIELDS.into_iter().zip(originals) {
            assert_eq!(super::snapshot(p, key).unwrap(), original);
        }
        assert!(document(&p.join("user.js")).unwrap().contains("unrelated"));
        assert_eq!(
            fs::read_to_string(p.join("cookies.sqlite")).unwrap(),
            "login data"
        );
        assert!(!p.join("nodecloak-geolocation-original.json").exists());
    }
    #[test]
    fn native_geolocation_rejects_tampered_backups_without_changing_preferences() {
        let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = dir.path();
        fs::write(p.join("user.js"), "user_pref(\"unrelated\", 42);\n").unwrap();
        let before = fs::read(p.join("user.js")).unwrap();
        fs::write(
            p.join("nodecloak-geolocation-original.json"),
            r#"[["security.sandbox.content.level",{"user":0,"runtime":0}]]"#,
        )
        .unwrap();
        assert!(prepare_geolocation(p, None).is_err());
        assert_eq!(fs::read(p.join("user.js")).unwrap(), before);
    }
    #[test]
    fn dedicated_startup_overrides_homepage_without_changing_dns_or_login_data() {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = temp.path().join("new profile");
        prepare_startup(&p).unwrap();
        fs::write(p.join("cookies.sqlite"), "login data").unwrap();
        fs::write(p.join("user.js"), "user_pref(\"browser.startup.homepage\", \"https://home.firefoxchina.cn\");\nuser_pref(\"network.trr.mode\", 3);\nuser_pref(\"font.system.whitelist\", \"Arial\");\n").unwrap();
        prepare_startup(&p).unwrap();
        prepare_startup(&p).unwrap();
        let text = document(&p.join("user.js")).unwrap();
        assert_eq!(
            value_in(&text, "browser.startup.homepage").unwrap(),
            Some(json!("about:blank"))
        );
        assert_eq!(value_in(&text, "network.trr.mode").unwrap(), Some(json!(3)));
        assert_eq!(
            value_in(&text, "font.system.whitelist").unwrap(),
            Some(json!("Arial"))
        );
        assert!(!text.contains("firefoxchina.cn"));
        assert_eq!(
            fs::read_to_string(p.join("cookies.sqlite")).unwrap(),
            "login data"
        );
        assert_eq!(
            text.matches("user_pref(\"browser.startup.homepage\",")
                .count(),
            1
        );
    }
    #[test]
    fn restores_runtime_and_startup_values_without_erasing_unrelated_changes() {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = temp.path();
        fs::write(p.join("prefs.js"), "// keep\nuser_pref(\"font.system.whitelist\", \"Arial,SimSun\");\nuser_pref(\"unrelated\", 42);\n").unwrap();
        let before = snapshot(p, "font.system.whitelist").unwrap();
        let after = json!({"user":FONT_LIST,"runtime":FONT_LIST});
        let changes = vec![("font.system.whitelist".into(), before.clone(), after)];
        apply(p, &changes, false).unwrap();
        assert!(configured(p, "fonts").unwrap());
        fs::write(
            p.join("user.js"),
            format!(
                "{}user_pref(\"new.other\", true);\n",
                document(&p.join("user.js")).unwrap()
            ),
        )
        .unwrap();
        apply(p, &changes, true).unwrap();
        assert_eq!(snapshot(p, "font.system.whitelist").unwrap(), before);
        assert!(document(&p.join("prefs.js")).unwrap().contains("unrelated"));
        assert!(document(&p.join("user.js")).unwrap().contains("new.other"));
        assert!(!document(&p.join("user.js"))
            .unwrap()
            .contains("font.system.whitelist"));
    }
    #[test]
    fn malformed_duplicate_or_conflicting_preferences_are_preserved() {
        for bad in ["user_pref(\"font.system.whitelist\", function(){});", "user_pref(\"font.system.whitelist\", \"Arial\");\nuser_pref(\"font.system.whitelist\", \"Calibri\");", "user_pref(\"font.system.whitelist\",\n\"Arial\");"] { assert!(value_in(bad,"font.system.whitelist").is_err()); }
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = temp.path();
        fs::write(
            p.join("user.js"),
            "user_pref(\"font.system.whitelist\", \"My newer choice\");\n",
        )
        .unwrap();
        let original = fs::read(p.join("user.js")).unwrap();
        assert!(apply(
            p,
            &[(
                "font.system.whitelist".into(),
                json!({"user":null,"runtime":null}),
                json!({"user":FONT_LIST,"runtime":FONT_LIST})
            )],
            true
        )
        .is_err());
        assert_eq!(fs::read(p.join("user.js")).unwrap(), original);
        assert!(!p.join("prefs.js").exists());
    }
    #[test]
    fn omitted_runtime_defaults_and_unrelated_strings_survive_restore() {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let p = temp.path();
        fs::write(p.join("prefs.js"),"user_pref(\"intl.regional_prefs.use_os_locales\", true);\nuser_pref(\"other\", \"font.system.whitelist\");\n").unwrap();
        let before = snapshot(p, "intl.regional_prefs.use_os_locales").unwrap();
        let changes = vec![(
            "intl.regional_prefs.use_os_locales".into(),
            before.clone(),
            json!({"user":false,"runtime":false}),
        )];
        apply(p, &changes, false).unwrap();
        fs::write(
            p.join("prefs.js"),
            "user_pref(\"other\", \"font.system.whitelist\");\n",
        )
        .unwrap();
        apply(p, &changes, true).unwrap();
        assert_eq!(
            snapshot(p, "intl.regional_prefs.use_os_locales").unwrap(),
            before
        );
        assert!(document(&p.join("prefs.js"))
            .unwrap()
            .contains("\"other\", \"font.system.whitelist\""));
        let font_before = snapshot(p, "font.system.whitelist").unwrap();
        apply(
            p,
            &[(
                "font.system.whitelist".into(),
                font_before,
                json!({"user":FONT_LIST,"runtime":FONT_LIST}),
            )],
            false,
        )
        .unwrap();
        assert!(document(&p.join("prefs.js"))
            .unwrap()
            .contains("\"other\", \"font.system.whitelist\""));
    }
}
