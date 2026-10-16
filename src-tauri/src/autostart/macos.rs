// launchctl's persistent override matters on pre-Ventura systems.
fn disabled_override(text: &str, label: &str) -> Result<bool, String> {
    if !text.contains("disabled services = {") {
        return Err("无法读取系统启动项禁用状态".into());
    }
    for line in text.lines() {
        if let Some((name, value)) = line.split_once("=>") {
            if serde_json::from_str::<String>(name.trim()).ok().as_deref() == Some(label) {
                return match value.trim().trim_end_matches(',') {
                    "true" => Ok(true),
                    "false" => Ok(false),
                    _ => Err("系统启动项禁用状态格式无效".into()),
                };
            }
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
mod system {
    use smappservice_rs::{AppService, ServiceStatus, ServiceType};
    use tauri_plugin_autostart::AutoLaunchManager;
    fn output(program: &str, args: &[&str]) -> Result<String, String> {
        let result = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|error| error.to_string())?;
        if !result.status.success() {
            return Err(format!(
                "{program}: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        String::from_utf8(result.stdout).map_err(|error| error.to_string())
    }
    fn modern() -> Result<bool, String> {
        static MODERN: std::sync::OnceLock<Result<bool, String>> = std::sync::OnceLock::new();
        MODERN
            .get_or_init(|| {
                output("/usr/bin/sw_vers", &["-productVersion"])?
                    .trim()
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .parse::<u32>()
                    .map(|major| major >= 13)
                    .map_err(|error| error.to_string())
            })
            .clone()
    }
    fn domain() -> Result<String, String> {
        let uid = output("/usr/bin/id", &["-u"])?;
        let uid = uid
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        Ok(format!("gui/{uid}"))
    }
    fn approval() -> String {
        "请在系统设置 → 通用 → 登录项中允许 NodeCloak 自动启动。".into()
    }
    pub(crate) fn is_enabled(fallback: &AutoLaunchManager) -> Result<bool, String> {
        if modern()? && !cfg!(debug_assertions) {
            return Ok(AppService::new(ServiceType::MainApp).status() == ServiceStatus::Enabled);
        }
        if !fallback.is_enabled().map_err(|error| error.to_string())? {
            return Ok(false);
        }
        Ok(!super::disabled_override(
            &output("/bin/launchctl", &["print-disabled", &domain()?])?,
            super::super::entry_name(),
        )?)
    }
    pub(crate) fn enable(fallback: &AutoLaunchManager) -> Result<(), String> {
        if modern()? && !cfg!(debug_assertions) {
            let service = AppService::new(ServiceType::MainApp);
            match service.status() {
                ServiceStatus::Enabled => return Ok(()),
                ServiceStatus::RequiresApproval => return Err(approval()),
                _ => (),
            }
            service.register().map_err(|error| error.to_string())?;
            return if service.status() == ServiceStatus::Enabled {
                Ok(())
            } else {
                Err(approval())
            };
        }
        fallback.enable().map_err(|error| error.to_string())?;
        output(
            "/bin/launchctl",
            &[
                "enable",
                &format!("{}/{}", domain()?, super::super::entry_name()),
            ],
        )?;
        Ok(())
    }
    pub(crate) fn disable(fallback: &AutoLaunchManager) -> Result<(), String> {
        if modern()? && !cfg!(debug_assertions) {
            let service = AppService::new(ServiceType::MainApp);
            return match service.status() {
                ServiceStatus::NotRegistered | ServiceStatus::NotFound => Ok(()),
                _ => service.unregister().map_err(|error| error.to_string()),
            };
        }
        fallback.disable().map_err(|error| error.to_string())
    }
}
#[cfg(target_os = "macos")]
pub(super) use system::{disable, enable, is_enabled};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_uses_the_exact_launch_agent_override() {
        let output =
            "disabled services = {\n \"NodeCloak\" => true\n \"NodeCloak Development\" => false\n}";
        assert!(disabled_override(output, "NodeCloak").unwrap());
        assert!(!disabled_override(output, "NodeCloak Development").unwrap());
        assert!(!disabled_override(output, "Other").unwrap());
        assert!(disabled_override("unavailable", "NodeCloak").is_err());
    }
}
