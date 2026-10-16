use std::path::Path;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_autostart::{AutoLaunchManager, ManagerExt};
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(windows)]
mod windows;
pub fn entry_name() -> &'static str {
    if cfg!(debug_assertions) {
        "NodeCloak Development"
    } else {
        "NodeCloak"
    }
}

#[derive(Default)]
pub struct StartupState(Mutex<()>);

trait Backend {
    fn is_enabled(&self) -> Result<bool, String>;
    fn enable(&self) -> Result<(), String>;
    fn disable(&self) -> Result<(), String>;
}

impl Backend for AutoLaunchManager {
    fn is_enabled(&self) -> Result<bool, String> {
        #[cfg(windows)]
        return windows::is_enabled();
        #[cfg(target_os = "macos")]
        return macos::is_enabled(self);
        #[cfg(not(any(windows, target_os = "macos")))]
        AutoLaunchManager::is_enabled(self).map_err(|error| error.to_string())
    }
    fn enable(&self) -> Result<(), String> {
        #[cfg(windows)]
        return windows::enable();
        #[cfg(target_os = "macos")]
        return macos::enable(self);
        #[cfg(not(any(windows, target_os = "macos")))]
        AutoLaunchManager::enable(self).map_err(|error| error.to_string())
    }
    fn disable(&self) -> Result<(), String> {
        #[cfg(windows)]
        return windows::disable();
        #[cfg(target_os = "macos")]
        return macos::disable(self);
        #[cfg(not(any(windows, target_os = "macos")))]
        AutoLaunchManager::disable(self).map_err(|error| error.to_string())
    }
}

fn configured(root: &Path) -> std::path::PathBuf {
    root.join(if cfg!(debug_assertions) {
        "autostart-development-configured-v1"
    } else {
        "autostart-configured-v1"
    })
}
fn remember(root: &Path) -> Result<(), String> {
    crate::engine::atomic_write(&configured(root), b"configured\n")
}
fn initialize(root: &Path, backend: &impl Backend) -> Result<(), String> {
    // Only apply the default once. Later OS-level opt-outs are respected too.
    if configured(root)
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    if !backend.is_enabled()? {
        backend.enable()?;
    }
    if !backend.is_enabled()? {
        return Err("系统未启用开机自动启动".into());
    }
    remember(root)
}
fn change(root: &Path, backend: &impl Backend, enabled: bool) -> Result<bool, String> {
    // Record explicit intent before touching the OS, so a failed opt-out is
    // never mistaken for an untouched install on the next launch.
    remember(root)?;
    if enabled {
        backend.enable()?;
    } else {
        backend.disable()?;
    }
    let actual = backend.is_enabled()?;
    if actual != enabled {
        return Err("系统未应用开机自动启动设置".into());
    }
    Ok(actual)
}

pub fn initialize_app(app: &tauri::AppHandle) -> Result<(), String> {
    // Development executables must not replace the installed app's login item.
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let state = app.state::<StartupState>();
    let _lock = state.0.lock().map_err(|error| error.to_string())?;
    initialize(
        &app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
        &*app.autolaunch(),
    )
}

#[tauri::command]
pub async fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<StartupState>();
        let _lock = state.0.lock().map_err(|error| error.to_string())?;
        Backend::is_enabled(&*app.autolaunch())
    })
    .await
    .map_err(|error| error.to_string())?
}
#[tauri::command]
pub async fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<StartupState>();
        let _lock = state.0.lock().map_err(|error| error.to_string())?;
        change(
            &app.path()
                .app_config_dir()
                .map_err(|error| error.to_string())?,
            &*app.autolaunch(),
            enabled,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[derive(Default)]
    struct System {
        enabled: Cell<bool>,
        fail: Cell<bool>,
        ignore: Cell<bool>,
    }
    impl Backend for System {
        fn is_enabled(&self) -> Result<bool, String> {
            Ok(self.enabled.get())
        }
        fn enable(&self) -> Result<(), String> {
            if self.fail.get() {
                return Err("registration denied".into());
            }
            if !self.ignore.get() {
                self.enabled.set(true);
            }
            Ok(())
        }
        fn disable(&self) -> Result<(), String> {
            if self.fail.get() {
                return Err("registration denied".into());
            }
            self.enabled.set(false);
            Ok(())
        }
    }
    #[test]
    fn fresh_install_enables_login_startup_and_preserves_later_opt_out() {
        let root = tempfile::tempdir().unwrap();
        let system = System::default();
        initialize(root.path(), &system).unwrap();
        assert!(system.enabled.get());
        assert!(!change(root.path(), &system, false).unwrap());
        initialize(root.path(), &system).unwrap();
        assert!(!system.enabled.get());
    }
    #[test]
    fn failed_default_registration_can_retry_without_marking_success() {
        let root = tempfile::tempdir().unwrap();
        let system = System::default();
        system.fail.set(true);
        assert!(initialize(root.path(), &system).is_err());
        system.fail.set(false);
        initialize(root.path(), &system).unwrap();
        assert!(system.enabled.get());
    }
    #[test]
    fn does_not_report_success_when_system_ignores_registration() {
        let root = tempfile::tempdir().unwrap();
        let system = System::default();
        system.ignore.set(true);
        assert!(change(root.path(), &system, true).is_err());
        assert!(!system.enabled.get());
    }
    #[test]
    fn explicit_opt_out_and_external_disabling_are_not_overridden() {
        let root = tempfile::tempdir().unwrap();
        let system = System::default();
        change(root.path(), &system, false).unwrap();
        initialize(root.path(), &system).unwrap();
        assert!(!system.enabled.get());
        change(root.path(), &system, true).unwrap();
        system.enabled.set(false);
        initialize(root.path(), &system).unwrap();
        assert!(!system.enabled.get());
    }
}
