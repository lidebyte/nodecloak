//! Narrow Native Messaging contract for the Firefox profile privacy extension.
use std::{
    io::{Read, Write},
    path::Path,
};

const SETTINGS_FILE: &str = "nodecloak-fingerprint-extension.json";
const MAX_MESSAGE: usize = 4096;

#[derive(serde::Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Request {
    #[serde(rename = "get_settings")]
    GetSettings,
}

/// Firefox passes the native manifest path and the calling add-on ID. Dispatch
/// before starting Tauri so this process never opens a UI or uses app state.
pub fn is_invocation(args: &[String]) -> bool {
    args.len() == 3
        && Path::new(&args[1]).is_absolute()
        && Path::new(&args[1])
            .file_name()
            .is_some_and(|n| n == "com.nodecloak.fingerprint.json")
        && args[2] == "fingerprint@nodecloak.com"
}

pub fn run_stdio() -> Result<(), String> {
    let profile = std::env::var_os("NODECLOAK_FIREFOX_PROFILE")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("FIREFOX_PROFILE_NOT_CONFIGURED")?;
    crate::profiles::guard_tree(&profile)?;
    crate::browser_control::log_startup_endpoint(
        &profile,
        "firefox",
        "native_host_started",
        "profile privacy host started",
    );
    let result = exchange(
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &profile,
    )
    .map_err(str::to_owned);
    let (event, detail) = if result.is_ok() {
        ("native_host_ready", "validated profile settings returned")
    } else {
        (
            "native_host_failed",
            "profile privacy host request or settings validation failed",
        )
    };
    crate::browser_control::log_startup_endpoint(&profile, "firefox", event, detail);
    result
}

fn exchange(
    input: &mut impl Read,
    output: &mut impl Write,
    profile: &Path,
) -> Result<(), &'static str> {
    let mut header = [0; 4];
    input
        .read_exact(&mut header)
        .map_err(|_| "INVALID_NATIVE_MESSAGE")?;
    let length = u32::from_ne_bytes(header) as usize;
    if length == 0 || length > MAX_MESSAGE {
        return Err("INVALID_NATIVE_MESSAGE");
    }
    let mut bytes = vec![0; length];
    input
        .read_exact(&mut bytes)
        .map_err(|_| "INVALID_NATIVE_MESSAGE")?;
    let _: Request = serde_json::from_slice(&bytes).map_err(|_| "INVALID_NATIVE_MESSAGE")?;
    let file = profile.join(SETTINGS_FILE);
    crate::profiles::guard_tree(&file).map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    let opened = std::fs::File::open(&file).map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    let metadata = opened.metadata().map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    if !metadata.is_file() || metadata.len() > MAX_MESSAGE as u64 {
        return Err("INVALID_PROFILE_SETTINGS");
    }
    let mut content = Vec::new();
    opened
        .take(MAX_MESSAGE as u64 + 1)
        .read_to_end(&mut content)
        .map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    if content.len() > MAX_MESSAGE {
        return Err("INVALID_PROFILE_SETTINGS");
    }
    let settings: crate::fingerprint::Settings =
        serde_json::from_slice(&content).map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    crate::fingerprint::validate(&settings).map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    if !settings.enabled {
        return Err("INVALID_PROFILE_SETTINGS");
    }
    let response = serde_json::to_vec(&serde_json::json!({"fingerprint":settings}))
        .map_err(|_| "INVALID_PROFILE_SETTINGS")?;
    output
        .write_all(&(response.len() as u32).to_ne_bytes())
        .map_err(|_| "NATIVE_WRITE_FAILED")?;
    output
        .write_all(&response)
        .map_err(|_| "NATIVE_WRITE_FAILED")?;
    output.flush().map_err(|_| "NATIVE_WRITE_FAILED")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    fn request(value: serde_json::Value) -> Vec<u8> {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut result = (bytes.len() as u32).to_le_bytes().to_vec();
        result.extend(bytes);
        result
    }
    fn response(bytes: &[u8]) -> serde_json::Value {
        let length = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), length + 4);
        serde_json::from_slice(&bytes[4..]).unwrap()
    }
    #[test]
    fn native_host_returns_only_validated_profile_settings() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().canonicalize().unwrap();
        let settings = crate::fingerprint::fresh().unwrap();
        std::fs::write(
            profile.join("nodecloak-fingerprint-extension.json"),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        let mut output = Vec::new();
        exchange(
            &mut Cursor::new(request(json!({"type":"get_settings"}))),
            &mut output,
            &profile,
        )
        .unwrap();
        assert_eq!(response(&output), json!({"fingerprint":settings}));
    }
    #[test]
    fn native_host_rejects_unknown_or_oversized_requests_without_loading_settings() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().canonicalize().unwrap();
        for bytes in [
            request(json!({"type":"get_settings","path":"cookies.sqlite"})),
            request(json!({"type":"evaluate","code":"alert(1)"})),
            (1024u32 * 1024).to_le_bytes().to_vec(),
            vec![0, 0, 0, 0],
            vec![4, 0, 0, 0, b'{'],
        ] {
            let mut output = Vec::new();
            assert!(exchange(&mut Cursor::new(bytes), &mut output, &profile).is_err());
            assert!(output.is_empty());
        }
    }
    #[test]
    fn native_host_fails_closed_for_missing_disabled_future_or_corrupt_profile_settings() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().canonicalize().unwrap();
        let settings = crate::fingerprint::fresh().unwrap();
        let valid = serde_json::to_value(&settings).unwrap();
        for value in [
            None,
            Some(json!({})),
            Some({
                let mut v = valid.clone();
                v["enabled"] = json!(false);
                v
            }),
            Some({
                let mut v = valid.clone();
                v["version"] = json!(2);
                v
            }),
        ] {
            let file = profile.join("nodecloak-fingerprint-extension.json");
            if let Some(value) = value {
                std::fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            let mut output = Vec::new();
            assert!(exchange(
                &mut Cursor::new(request(json!({"type":"get_settings"}))),
                &mut output,
                &profile
            )
            .is_err());
            assert!(output.is_empty());
        }
    }
    #[test]
    fn native_host_dispatch_does_not_capture_regular_app_or_mcp_arguments() {
        let manifest = std::env::temp_dir()
            .join("com.nodecloak.fingerprint.json")
            .to_string_lossy()
            .to_string();
        assert!(is_invocation(&[
            "nodecloak".into(),
            manifest.clone(),
            "fingerprint@nodecloak.com".into()
        ]));
        for args in [
            vec!["nodecloak".into()],
            vec!["nodecloak".into(), "--mcp".into()],
            vec![
                "nodecloak".into(),
                manifest.clone(),
                "other@example.test".into(),
            ],
            vec![
                "nodecloak".into(),
                "relative.json".into(),
                "fingerprint@nodecloak.com".into(),
            ],
        ] {
            assert!(!is_invocation(&args));
        }
    }
}
