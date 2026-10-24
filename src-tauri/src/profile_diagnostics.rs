//! Read only bounded, structured diagnostics for the selected visible profile.
use crate::profiles;
use serde::Serialize;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub profile_id: String,
    pub name: String,
    pub log_path: String,
    pub entries: Vec<Entry>,
}
#[derive(Serialize)]
pub struct Entry {
    time: String,
    browser: String,
    event: String,
    attempt: u64,
    detail: String,
}

fn read_entries(path: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for name in [
        "nodecloak-control.previous.jsonl",
        "nodecloak-control.jsonl",
    ] {
        let file = path.join(name);
        profiles::guard_tree(&file)?;
        let mut input = match std::fs::File::open(&file) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("无法读取副本启动日志".into()),
        };
        if !input
            .metadata()
            .map_err(|_| "无法读取副本启动日志")?
            .is_file()
        {
            return Err("副本启动日志不是普通文件".into());
        }
        let length = input.metadata().map_err(|_| "无法读取副本启动日志")?.len();
        let start = length.saturating_sub(256 * 1024);
        input
            .seek(SeekFrom::Start(start))
            .map_err(|_| "无法读取副本启动日志")?;
        let mut bytes = Vec::new();
        input
            .take(256 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|_| "无法读取副本启动日志")?;
        let text = String::from_utf8_lossy(&bytes);
        for line in text.lines().skip(usize::from(start > 0)) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let field = |key: &str, limit| {
                v[key]
                    .as_str()
                    .map(|s| s.chars().take(limit).collect::<String>())
            };
            let (Some(time), Some(browser), Some(event), Some(detail)) = (
                field("time", 64),
                field("browser", 16),
                field("event", 80),
                field("detail", 2048),
            ) else {
                continue;
            };
            entries.push(Entry {
                time,
                browser,
                event,
                detail,
                attempt: v["attempt"].as_u64().unwrap_or(0),
            });
        }
    }
    if entries.len() > 200 {
        entries.drain(..entries.len() - 200);
    }
    Ok(entries)
}
pub fn read(root: &Path, id: &str) -> Result<Report, String> {
    let profile = profiles::get(root, id, true)?;
    if !crate::cloud::sharing::visible(root, id)? {
        return Err("此副本当前不可用".into());
    }
    let path = profiles::path(root, &profile)?;
    Ok(Report {
        profile_id: profile.id,
        name: profile.name,
        log_path: path
            .join("nodecloak-control.jsonl")
            .to_string_lossy()
            .into_owned(),
        entries: read_entries(&path)?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn line(event: &str) -> String {
        serde_json::json!({"time":"2026-10-25T00:00:00Z","browser":"edge","event":event,"attempt":0,"detail":"safe diagnostic","unknown":"not exposed"}).to_string()+"\n"
    }
    #[test]
    fn missing_logs_are_empty_and_rotation_retains_only_recent_structured_entries() {
        let d = tempfile::tempdir().unwrap();
        assert!(read_entries(d.path()).unwrap().is_empty());
        std::fs::write(
            d.path().join("nodecloak-control.previous.jsonl"),
            line("previous"),
        )
        .unwrap();
        std::fs::write(
            d.path().join("nodecloak-control.jsonl"),
            line("current") + "partial {",
        )
        .unwrap();
        let entries = read_entries(d.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].event, "previous");
        assert_eq!(entries[1].event, "current");
        assert!(!serde_json::to_string(&entries)
            .unwrap()
            .contains("not exposed"));
        std::fs::write(
            d.path().join("nodecloak-control.jsonl"),
            line("latest").repeat(500),
        )
        .unwrap();
        assert_eq!(read_entries(d.path()).unwrap().len(), 200);
    }
    #[test]
    fn bounded_tail_skips_partial_and_non_record_data() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("nodecloak-control.jsonl"),
            "x".repeat(300 * 1024) + "\n" + &line("last"),
        )
        .unwrap();
        let entries = read_entries(d.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event, "last");
    }
    #[test]
    fn only_registry_profiles_can_select_logs() {
        let d = tempfile::tempdir().unwrap();
        assert!(read(d.path(), "../other").is_err());
        assert!(read(d.path(), &"a".repeat(32)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_logs_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("nodecloak-control.jsonl"))
            .unwrap();
        assert!(read_entries(d.path()).is_err());
    }
}
