//! Local browser-owned session state. Never exported as page markup or synced.
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn normal_tabs(targets: &serde_json::Value, normal: &str) -> Option<Vec<String>> {
    let pages = targets["targetInfos"]
        .as_array()?
        .iter()
        .filter(|target| {
            target["type"] == "page"
                && target["browserContextId"].as_str() == Some(normal)
                && target["subtype"].is_null()
        })
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return None;
    }
    Some(
        pages
            .iter()
            .filter_map(|target| target["url"].as_str())
            .map(str::to_owned)
            .collect(),
    )
}

fn tab_url(value: &str) -> bool {
    value.len() <= 16384
        && reqwest::Url::parse(value).is_ok_and(|u| matches!(u.scheme(), "http" | "https" | "file"))
}
pub(crate) fn read_tabs(profile: &Path) -> Result<Vec<String>, String> {
    let file = profile.join("nodecloak-open-tabs.json");
    crate::profiles::guard_tree(&file)?;
    if !file.exists() {
        return Ok(Vec::new());
    }
    if std::fs::metadata(&file)
        .map_err(|_| "无法读取上次标签页")?
        .len()
        > 512 * 1024
    {
        return Err("上次标签页记录过大，原文件已保留".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(file).map_err(|_| "无法读取上次标签页")?)
            .map_err(|_| "上次标签页记录无效，原文件已保留")?;
    if value["schema"] != 1 {
        return Err("上次标签页记录版本无效".into());
    }
    let tabs = value["tabs"]
        .as_array()
        .filter(|tabs| tabs.len() <= 128)
        .ok_or("上次标签页记录无效")?;
    tabs.iter()
        .map(|v| {
            v.as_str()
                .filter(|url| tab_url(url))
                .map(str::to_owned)
                .ok_or("上次标签页地址无效".into())
        })
        .collect()
}
pub(crate) fn write_tabs(profile: &Path, urls: &[String]) -> Result<(), String> {
    let tabs: Vec<_> = urls
        .iter()
        .filter(|url| tab_url(url))
        .take(128)
        .cloned()
        .collect();
    let file = profile.join("nodecloak-open-tabs.json");
    crate::profiles::guard_tree(&file)?;
    let bytes =
        serde_json::to_vec(&json!({"schema":1,"tabs":tabs})).map_err(|_| "无法保存上次标签页")?;
    if bytes.len() > 512 * 1024 {
        return Err("上次标签页记录过大".into());
    }
    if std::fs::read(&file).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    crate::engine::atomic_write(&file, &bytes)
}

const FIREFOX_FILES: [&str; 2] = ["sessionstore.jsonlz4", "sessionstore-backups"];
const FIREFOX_SOURCES: [&str; 4] = [
    "sessionstore.jsonlz4",
    "sessionstore-backups/recovery.jsonlz4",
    "sessionstore-backups/recovery.baklz4",
    "sessionstore-backups/previous.jsonlz4",
];
fn upgrade_source(value: &str) -> bool {
    value
        .strip_prefix("sessionstore-backups/upgrade.jsonlz4-")
        .is_some_and(|id| (14..=32).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_digit()))
}
fn native_sources(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for name in FIREFOX_SOURCES {
        let file = directory.join(name);
        if valid_firefox_file(&file)? {
            files.push(file);
        }
    }
    let backups = directory.join("sessionstore-backups");
    crate::profiles::guard_tree(&backups)?;
    if backups.exists() {
        let mut upgrades = Vec::new();
        for entry in std::fs::read_dir(backups).map_err(|_| "无法读取 Firefox 升级会话备份")?
        {
            let entry = entry.map_err(|_| "无法读取 Firefox 升级会话备份")?;
            let name = format!(
                "sessionstore-backups/{}",
                entry.file_name().to_string_lossy()
            );
            if upgrade_source(&name) && valid_firefox_file(&entry.path())? {
                upgrades.push(entry.path());
                upgrades.sort_by(|a, b| b.cmp(a));
                upgrades.truncate(3);
            }
        }
        files.extend(upgrades);
    }
    Ok(files)
}
pub(crate) struct FirefoxSession {
    profile: PathBuf,
    directory: PathBuf,
    token: String,
    done: bool,
}
impl FirefoxSession {
    pub(crate) fn park(profile: &Path) -> Result<Option<Self>, String> {
        crate::platform::ensure_profile_closed(profile)?;
        Self::recover(profile)?;
        let sources = native_sources(profile)?;
        let Some(first) = sources.first() else {
            return Ok(None);
        };
        let source = first
            .strip_prefix(profile)
            .map_err(|_| "Firefox 会话路径无效")?
            .to_string_lossy()
            .replace('\\', "/");
        let directory = profile.join("nodecloak-session-restore");
        crate::profiles::guard_tree(&directory)?;
        let token = crate::fingerprint::seed()?;
        std::fs::create_dir(&directory).map_err(|_| "无法保存上次 Firefox 会话")?;
        let saved = Self {
            profile: profile.into(),
            directory,
            token,
            done: false,
        };
        crate::engine::atomic_write(
            &saved.directory.join("intent.json"),
            &serde_json::to_vec(&json!({"schema":1,"source":source,"token":saved.token}))
                .map_err(|_| "会话快照无效")?,
        )?;
        for name in FIREFOX_FILES {
            let original = profile.join(name);
            crate::profiles::guard_tree(&original)?;
            if original.exists() {
                std::fs::rename(&original, saved.directory.join(name))
                    .map_err(|_| "无法暂存上次 Firefox 会话")?;
            }
        }
        Ok(Some(saved))
    }
    pub(crate) fn sources(&self) -> Result<Vec<PathBuf>, String> {
        native_sources(&self.directory)
    }
    pub(crate) fn receipt(&self) -> PathBuf {
        self.directory.join("restored.json")
    }
    pub(crate) fn token(&self) -> &str {
        &self.token
    }
    pub(crate) fn finish_without_restore(&mut self) -> Result<(), String> {
        crate::profiles::guard_tree(&self.receipt())?;
        crate::engine::atomic_write(
            &self.receipt(),
            &serde_json::to_vec(&json!({"schema":1,"token":self.token,"status":"retained"}))
                .map_err(|_| "会话恢复状态无效")?,
        )?;
        self.retain()
    }
    pub(crate) fn commit(&mut self) -> Result<bool, String> {
        let receipt = self.receipt();
        crate::profiles::guard_tree(&receipt)?;
        if !receipt.exists() {
            return Ok(false);
        }
        if std::fs::metadata(&receipt)
            .map_err(|_| "无法读取会话恢复状态")?
            .len()
            > 1024
        {
            return Err("会话恢复状态无效".into());
        }
        let bytes = std::fs::read(receipt).map_err(|_| "无法读取会话恢复状态")?;
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(false);
        };
        if value["schema"] != 1 || value["token"] != self.token {
            return Err("会话恢复状态无效".into());
        }
        if value["status"] == "pending" {
            return Ok(false);
        }
        if value["status"] == "retained" {
            self.retain()?;
            return Ok(true);
        }
        if value["status"] == "failed" {
            return Err(format!(
                "Firefox 会话恢复未完成（{} / {}），原始快照已保留",
                value["stage"].as_str().unwrap_or("unknown"),
                value["reason"].as_str().unwrap_or("unknown")
            ));
        }
        let name = value["file"]
            .as_str()
            .filter(|name| FIREFOX_SOURCES.contains(name))
            .ok_or("会话恢复文件无效")?;
        let digest = value["sha256"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("会话恢复校验值无效")?;
        let persisted = self.profile.join(name);
        if !valid_firefox_file(&persisted)? {
            self.finish_without_restore()?;
            return Ok(true);
        }
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(persisted).map_err(|_| "无法核对已恢复会话")?;
        let actual = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        // Firefox may save again after the receipt. Keep originals until a
        // later successful restore rather than trusting unrelated newer data.
        if actual != digest {
            self.finish_without_restore()?;
            return Ok(true);
        }
        crate::profiles::guard_tree(&self.directory)?;
        self.retain()?;
        Ok(true)
    }
    fn retain(&mut self) -> Result<(), String> {
        crate::profiles::guard_tree(&self.directory)?;
        let backup = self.profile.join("nodecloak-session-backup");
        crate::profiles::guard_tree(&backup)?;
        if backup.exists() {
            let intent = backup.join("intent.json");
            crate::profiles::guard_tree(&intent)?;
            if std::fs::metadata(&intent)
                .map_err(|_| "Firefox 会话备份记录缺失")?
                .len()
                > 4096
            {
                return Err("Firefox 会话备份记录过大".into());
            }
            let valid = std::fs::read(intent)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .is_some_and(|v| {
                    v["schema"] == 1
                        && v["token"].as_str().is_some_and(|s| {
                            s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
                        })
                });
            if !valid {
                return Err("Firefox 会话备份目录无效，原始快照已保留".into());
            }
            let receipt = backup.join("restored.json");
            crate::profiles::guard_tree(&receipt)?;
            let verified = std::fs::metadata(&receipt)
                .is_ok_and(|m| m.is_file() && m.len() <= 1024)
                && std::fs::read(&receipt)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .is_some_and(|v| {
                        v["schema"] == 1
                            && v["status"].is_null()
                            && v["sha256"].as_str().is_some_and(|s| {
                                s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
                            })
                    });
            if verified {
                std::fs::remove_dir_all(&backup).map_err(|_| "无法轮换 Firefox 会话备份")?;
            } else {
                // A previous save may have failed and left only a blank live
                // session. Keep that last good original across later launches.
                let recovery = self.profile.join(format!(
                    "nodecloak-session-recovery-{}",
                    uuid::Uuid::new_v4()
                ));
                crate::profiles::guard_tree(&recovery)?;
                std::fs::rename(&backup, recovery)
                    .map_err(|_| "无法保留未确认的 Firefox 会话备份")?;
            }
        }
        std::fs::rename(&self.directory, backup).map_err(|_| "无法保留 Firefox 会话备份")?;
        self.done = true;
        Ok(())
    }
    fn rollback(&mut self) -> Result<(), String> {
        crate::platform::ensure_profile_closed(&self.profile)?;
        let quarantine = self.profile.join(format!(
            "nodecloak-session-recovery-{}",
            uuid::Uuid::new_v4()
        ));
        for name in FIREFOX_FILES {
            let original = self.directory.join(name);
            crate::profiles::guard_tree(&original)?;
            if !original.exists() {
                continue;
            }
            let target = self.profile.join(name);
            crate::profiles::guard_tree(&target)?;
            if target.exists() {
                std::fs::create_dir_all(&quarantine).map_err(|_| "无法保留会话恢复记录")?;
                std::fs::rename(&target, quarantine.join(name))
                    .map_err(|_| "无法保留会话恢复记录")?;
            }
            std::fs::rename(original, target).map_err(|_| "无法恢复原始 Firefox 会话")?;
        }
        crate::profiles::guard_tree(&self.directory)?;
        std::fs::remove_dir_all(&self.directory).map_err(|_| "无法清理会话恢复记录")?;
        self.done = true;
        Ok(())
    }
    pub(crate) fn recover(profile: &Path) -> Result<(), String> {
        let directory = profile.join("nodecloak-session-restore");
        crate::profiles::guard_tree(&directory)?;
        if !directory.exists() {
            return Ok(());
        }
        let file = directory.join("intent.json");
        crate::profiles::guard_tree(&file)?;
        if std::fs::metadata(&file)
            .map_err(|_| "Firefox 会话恢复记录缺失，原数据已保留")?
            .len()
            > 4096
        {
            return Err("Firefox 会话恢复记录无效".into());
        }
        let intent: serde_json::Value =
            serde_json::from_slice(&std::fs::read(file).map_err(|_| "无法读取会话恢复记录")?)
                .map_err(|_| "Firefox 会话恢复记录无效")?;
        let _source = intent["source"]
            .as_str()
            .filter(|name| FIREFOX_SOURCES.contains(name) || upgrade_source(name))
            .ok_or("Firefox 会话恢复路径无效")?;
        let token = intent["token"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("Firefox 会话恢复记录无效")?
            .to_owned();
        if intent["schema"] != 1 {
            return Err("Firefox 会话恢复版本无效".into());
        }
        let mut saved = Self {
            profile: profile.into(),
            directory,
            token,
            done: false,
        };
        if !saved.commit()? {
            saved.rollback()?;
        }
        Ok(())
    }
}
impl Drop for FirefoxSession {
    fn drop(&mut self) {
        if !self.done && crate::platform::ensure_profile_closed(&self.profile).is_ok() {
            let _ = self.rollback();
        }
    }
}
fn valid_firefox_file(path: &Path) -> Result<bool, String> {
    crate::profiles::guard_tree(path)?;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("无法读取 Firefox 会话快照".into()),
    };
    let metadata = file.metadata().map_err(|_| "无法读取 Firefox 会话快照")?;
    if !metadata.is_file() || metadata.len() <= 8 || metadata.len() > 64 * 1024 * 1024 {
        return Ok(false);
    }
    // Decompression, optional native decryption and JSON validation happen in
    // Firefox's own parent context, trying each candidate in native order.
    Ok(true)
}

pub(crate) fn chromium_available(profile: &Path) -> Result<bool, String> {
    let directory = profile.join("Default/Sessions");
    crate::profiles::guard_tree(&directory)?;
    let entries = match std::fs::read_dir(&directory) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("无法读取上次浏览器会话".into()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| "无法读取上次浏览器会话")?;
        if !entry.file_name().to_string_lossy().starts_with("Tabs_") {
            continue;
        }
        crate::profiles::guard_tree(&entry.path())?;
        let metadata = entry.metadata().map_err(|_| "无法读取上次浏览器会话")?;
        if metadata.is_file() && metadata.len() > 8 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrade_only_firefox_state_is_fenced_and_valid_backups_follow_native_priority() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path();
        std::fs::create_dir(profile.join("sessionstore-backups")).unwrap();
        std::fs::write(
            profile.join("sessionstore-backups/upgrade.jsonlz4-20261027000000"),
            b"mozLz40\0upgrade state",
        )
        .unwrap();
        {
            let saved = FirefoxSession::park(profile).unwrap().unwrap();
            assert!(!profile.join("sessionstore-backups").exists());
            assert_eq!(saved.sources().unwrap().len(), 1);
            std::mem::forget(saved);
        }
        FirefoxSession::recover(profile).unwrap();
        assert!(profile
            .join("sessionstore-backups/upgrade.jsonlz4-20261027000000")
            .exists());
        assert!(!upgrade_source(
            "sessionstore-backups/upgrade.jsonlz4-../../other"
        ));
        assert!(!upgrade_source(
            "../sessionstore-backups/upgrade.jsonlz4-20261027000000"
        ));
    }
    #[test]
    fn a_later_blank_session_never_rotates_out_a_previous_unconfirmed_original() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path();
        std::fs::write(
            profile.join("sessionstore.jsonlz4"),
            b"mozLz40\0last good session A",
        )
        .unwrap();
        let mut a = FirefoxSession::park(profile).unwrap().unwrap();
        a.finish_without_restore().unwrap();
        std::fs::write(
            profile.join("sessionstore.jsonlz4"),
            b"mozLz40\0blank session B after failed save",
        )
        .unwrap();
        let mut b = FirefoxSession::park(profile).unwrap().unwrap();
        b.finish_without_restore().unwrap();
        let archives = std::fs::read_dir(profile)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("nodecloak-session-recovery-")
            })
            .collect::<Vec<_>>();
        assert_eq!(archives.len(), 1);
        assert_eq!(
            std::fs::read(archives[0].path().join("sessionstore.jsonlz4")).unwrap(),
            b"mozLz40\0last good session A"
        );
        assert_eq!(
            std::fs::read(profile.join("nodecloak-session-backup/sessionstore.jsonlz4")).unwrap(),
            b"mozLz40\0blank session B after failed save"
        );
    }
    #[test]
    fn only_existing_native_tab_history_triggers_resume() {
        let root = tempfile::tempdir().unwrap();
        assert!(!chromium_available(root.path()).unwrap());
        let sessions = root.path().join("Default/Sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("Session_1"), b"unrelated session data").unwrap();
        std::fs::write(sessions.join("Tabs_1"), b"SNSS1234").unwrap();
        assert!(!chromium_available(root.path()).unwrap());
        std::fs::write(sessions.join("Tabs_1"), b"SNSS1234native tab data").unwrap();
        assert!(chromium_available(root.path()).unwrap());
    }
    #[test]
    fn firefox_saved_tabs_stay_parked_during_temporary_setup_and_roll_back_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path();
        std::fs::write(
            profile.join("sessionstore.jsonlz4"),
            b"mozLz40\0original session",
        )
        .unwrap();
        std::fs::create_dir(profile.join("sessionstore-backups")).unwrap();
        std::fs::write(
            profile.join("sessionstore-backups/previous.jsonlz4"),
            b"original backup",
        )
        .unwrap();
        std::fs::write(profile.join("cookies.sqlite"), b"login state").unwrap();
        {
            let saved = FirefoxSession::park(profile).unwrap().unwrap();
            assert!(!profile.join("sessionstore.jsonlz4").exists());
            assert!(!profile.join("sessionstore-backups").exists());
            assert_eq!(
                std::fs::read(&saved.sources().unwrap()[0]).unwrap(),
                b"mozLz40\0original session"
            );
            std::fs::write(
                profile.join("sessionstore.jsonlz4"),
                b"temporary blank session",
            )
            .unwrap();
        }
        assert_eq!(
            std::fs::read(profile.join("sessionstore.jsonlz4")).unwrap(),
            b"mozLz40\0original session"
        );
        assert_eq!(
            std::fs::read(profile.join("cookies.sqlite")).unwrap(),
            b"login state"
        );
    }
    #[test]
    fn firefox_original_snapshot_is_kept_as_backup_after_matching_native_save() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path();
        std::fs::write(profile.join("sessionstore.jsonlz4"), b"mozLz40\0original").unwrap();
        let mut saved = FirefoxSession::park(profile).unwrap().unwrap();
        assert!(!saved.commit().unwrap());
        std::fs::create_dir(profile.join("sessionstore-backups")).unwrap();
        std::fs::write(
            profile.join("sessionstore-backups/recovery.jsonlz4"),
            b"mozLz40\0new saved session",
        )
        .unwrap();
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(b"mozLz40\0new saved session")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        std::fs::write(saved.receipt(),serde_json::to_vec(&serde_json::json!({"schema":1,"token":saved.token(),"file":"sessionstore-backups/recovery.jsonlz4","sha256":digest})).unwrap()).unwrap();
        assert!(saved.commit().unwrap());
        assert!(!profile.join("nodecloak-session-restore").exists());
        assert_eq!(
            std::fs::read(profile.join("nodecloak-session-backup/sessionstore.jsonlz4")).unwrap(),
            b"mozLz40\0original"
        );
        assert!(profile
            .join("sessionstore-backups/recovery.jsonlz4")
            .exists());
    }
    #[test]
    fn edge_tab_capture_excludes_all_other_contexts_and_distinguishes_empty_window_from_shutdown() {
        let targets = json!({"targetInfos":[
            {"type":"page","browserContextId":"normal","url":"https://public.test/"},
            {"type":"page","browserContextId":"native-incognito-not-enumerated","url":"https://private.test/"},
            {"type":"page","url":"https://ambiguous.test/"},
            {"type":"page","browserContextId":"normal","subtype":"prerender","url":"https://prefetched.test/"},
            {"type":"iframe","browserContextId":"normal","url":"https://frame.test/"}
        ]});
        assert_eq!(
            normal_tabs(&targets, "normal").unwrap(),
            vec!["https://public.test/"]
        );
        assert!(normal_tabs(&json!({"targetInfos":[]}), "normal").is_none());
        let root = tempfile::tempdir().unwrap();
        write_tabs(root.path(), &["https://public.test/".into()]).unwrap();
        let live_blank=normal_tabs(&json!({"targetInfos":[{"type":"page","browserContextId":"normal","url":"edge://newtab/"}]}),"normal").unwrap();
        write_tabs(root.path(), &live_blank).unwrap();
        assert!(read_tabs(root.path()).unwrap().is_empty());
    }
    #[test]
    fn firefox_interrupted_setup_recovery_and_nonrestoring_launch_preserve_original_data() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path();
        std::fs::write(profile.join("sessionstore.jsonlz4"), b"mozLz40\0original").unwrap();
        let saved = FirefoxSession::park(profile).unwrap().unwrap();
        std::mem::forget(saved);
        FirefoxSession::recover(profile).unwrap();
        assert_eq!(
            std::fs::read(profile.join("sessionstore.jsonlz4")).unwrap(),
            b"mozLz40\0original"
        );
        let mut saved = FirefoxSession::park(profile).unwrap().unwrap();
        saved.finish_without_restore().unwrap();
        assert!(!profile.join("sessionstore.jsonlz4").exists());
        assert!(profile
            .join("nodecloak-session-backup/sessionstore.jsonlz4")
            .exists());
    }
}
