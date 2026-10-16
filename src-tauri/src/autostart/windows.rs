use std::path::Path;
use winreg::{enums::*, RegKey, RegValue};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
fn name() -> &'static str {
    super::entry_name()
}
fn command(path: &Path) -> Result<String, String> {
    let path = path.to_str().ok_or("启动路径编码无效")?;
    if path.contains(['"', '\0', '\r', '\n']) {
        return Err("启动路径无效".into());
    }
    Ok(format!("\"{path}\""))
}
fn read(
    run: &RegKey,
    approved: Option<&RegKey>,
    name: &str,
    expected: &str,
) -> Result<bool, String> {
    let actual: String = match run.get_value(name) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if actual != expected {
        return Ok(false);
    }
    let value = match approved.map(|key| key.get_raw_value(name)) {
        None => return Ok(true),
        Some(Ok(value)) => value,
        Some(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Some(Err(error)) => return Err(error.to_string()),
    };
    match value.bytes.first() {
        Some(2 | 6) => Ok(true),
        Some(3 | 7) => Ok(false),
        _ => Err("系统启动项状态格式无效".into()),
    }
}
pub(super) fn is_enabled() -> Result<bool, String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let run = match user.open_subkey(RUN) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    let approved = match user.open_subkey(APPROVED) {
        Ok(key) => Some(key),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    read(
        &run,
        approved.as_ref(),
        name(),
        &command(&std::env::current_exe().map_err(|error| error.to_string())?)?,
    )
}
pub(super) fn enable() -> Result<(), String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = user.create_subkey(RUN).map_err(|error| error.to_string())?;
    let (approved, _) = user
        .create_subkey(APPROVED)
        .map_err(|error| error.to_string())?;
    run.set_value(
        name(),
        &command(&std::env::current_exe().map_err(|error| error.to_string())?)?,
    )
    .map_err(|error| error.to_string())?;
    approved
        .set_raw_value(
            name(),
            &RegValue {
                vtype: REG_BINARY,
                bytes: vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
        )
        .map_err(|error| error.to_string())
}
pub(super) fn disable() -> Result<(), String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    for path in [RUN, APPROVED] {
        match user
            .open_subkey_with_flags(path, KEY_SET_VALUE)
            .and_then(|key| key.delete_value(name()))
        {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_command_preserves_executable_paths_with_spaces() {
        assert_eq!(
            command(Path::new(r"C:\Program Files\NodeCloak\nodecloak.exe")).unwrap(),
            r#""C:\Program Files\NodeCloak\nodecloak.exe""#
        );
    }
    #[test]
    fn reads_task_manager_disabling_and_rejects_stale_executable_paths() {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let prefix = format!(
            r"Software\NodeCloak\AutostartTests\{}",
            uuid::Uuid::new_v4()
        );
        let (root, _) = user.create_subkey(&prefix).unwrap();
        let (run, _) = root.create_subkey("Run").unwrap();
        let (approved, _) = root.create_subkey("Approved").unwrap();
        let expected = r#""C:\Program Files\NodeCloak\nodecloak.exe""#;
        run.set_value("test-app", &expected).unwrap();
        assert!(read(&run, Some(&approved), "test-app", expected).unwrap());
        approved
            .set_raw_value(
                "test-app",
                &RegValue {
                    vtype: REG_BINARY,
                    bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                },
            )
            .unwrap();
        assert!(!read(&run, Some(&approved), "test-app", expected).unwrap());
        assert!(!read(
            &run,
            Some(&approved),
            "test-app",
            r#""D:\Moved App\nodecloak.exe""#
        )
        .unwrap());
        drop(approved);
        drop(run);
        drop(root);
        user.delete_subkey_all(prefix).unwrap();
    }
}
