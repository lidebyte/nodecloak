#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_file_is_written_and_flushed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.txt");
        write_recovery_file(&path, "synthetic recovery only").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "synthetic recovery only"
        );
        write_recovery_file(&path, "replacement").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn failed_write_is_reported_without_creating_another_destination() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            write_recovery_file(&dir.path().join("missing/recovery.txt"), "synthetic").is_err()
        );
        assert!(write_recovery_file(dir.path(), "synthetic").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
use super::{
    account::Connection,
    commands::{pending_two_factor_codes, Runtime},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Write, path::Path};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryFileKind {
    Vault,
    TwoFactor,
}

pub fn save(
    window: &tauri::WebviewWindow,
    c: &mut Connection,
    runtime: &Runtime,
    kind: RecoveryFileKind,
) -> Result<Value, String> {
    let account = c.disk.account.as_ref().ok_or("请先登录")?.email.clone();
    let (filename, text) = match kind {
        RecoveryFileKind::Vault => {
            let pending = c.pending_recovery()?;
            let code = pending["recoveryCode"].as_str().ok_or("恢复密钥不可用")?;
            ("NodeCloak-recovery-key.txt", Zeroizing::new(format!("NodeCloak recovery key\nAccount: {account}\n{code}\nKeep this key private. Losing all authorized devices and this key makes encrypted data unrecoverable.\n")))
        }
        RecoveryFileKind::TwoFactor => {
            let pending = pending_two_factor_codes(c, runtime)?;
            let codes = pending["backupCodes"]
                .as_array()
                .filter(|v| !v.is_empty())
                .ok_or("恢复码不可用")?;
            let mut text = Zeroizing::new(format!("NodeCloak 2FA recovery codes\n{account}\n\n"));
            for code in codes {
                text.push_str(code.as_str().ok_or("恢复码格式无效")?);
                text.push('\n');
            }
            text.push_str("\nEach code can be used once. Keep these codes private.\n");
            ("NodeCloak-2FA-recovery-codes.txt", text)
        }
    };
    let selected = window
        .dialog()
        .file()
        .set_parent(window)
        .set_title("NodeCloak · Save recovery file")
        .set_file_name(filename)
        .add_filter("Text", &["txt"])
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(json!({"saved":false}));
    };
    let path = selected.into_path().map_err(|_| "文件路径无效")?;
    write_recovery_file(&path, &text)?;
    Ok(json!({"saved":true,"path":path.to_string_lossy()}))
}

fn write_recovery_file(path: &Path, text: &str) -> Result<(), String> {
    let error = || "无法保存文件，请检查目录和写入权限。".to_string();
    if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
        return Err(error());
    }
    let parent = path.parent().ok_or_else(error)?;
    // A private temporary file replaces the selected file only after a successful flush.
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|_| error())?;
    file.write_all(text.as_bytes()).map_err(|_| error())?;
    file.as_file().sync_all().map_err(|_| error())?;
    file.persist(path).map_err(|_| error())?;
    Ok(())
}
