use super::{
    account::{required_string, Connection},
    sync, vault,
};
use crate::{data_root, platform, profiles, OperationLock};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct Runtime {
    pub oauth: Mutex<Option<Zeroizing<String>>>,
    pub two_factor: Mutex<Option<Zeroizing<String>>>,
    pub backup_codes: Mutex<Option<(String, Zeroizing<String>)>>,
}
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Action {
    RequestEmail {
        email: String,
    },
    VerifyEmail {
        email: String,
        code: String,
    },
    PasswordLogin {
        email: String,
        password: String,
    },
    VerifyTwoFactor {
        code: String,
    },
    CancelLogin,
    PendingTwoFactorRecovery,
    AcknowledgeTwoFactorRecovery,
    SetPassword {
        password: String,
        current_password: String,
        code: String,
    },
    GoogleUnlink {
        current_password: String,
        code: String,
    },
    StartTwoFactor {
        current_password: String,
        code: String,
    },
    EnableTwoFactor {
        code: String,
    },
    DisableTwoFactor {
        current_password: String,
        code: String,
    },
    GoogleStart,
    GooglePoll,
    Refresh,
    Initialize,
    PendingRecovery,
    SaveRecoveryFile { kind: super::recovery_export::RecoveryFileKind },
    AcknowledgeRecovery,
    Recover {
        code: String,
    },
    ConfirmDevice {
        code: String,
    },
    UpdateAccount {
        name: String,
        avatar: String,
    },
    Logout,
    Devices,
    ApproveDevice {
        id: String,
        verification_code: String,
    },
    RevokeDevice {
        id: String,
    },
    Preview,
    Setup {
        mode: String,
        local_ids: Vec<String>,
        cloud_ids: Vec<String>,
        sessions: bool,
    },
    Sync,
    Preferences {
        auto_sync: bool,
        session_sync: bool,
        pull_cloud: bool,
    },
    Resolve {
        id: String,
        kind: String,
        choice: String,
    },
    Detach {
        id: String,
        remove_local: bool,
    },
}
fn finish_primary_login(c: &mut Connection, runtime: &Runtime, v: Value) -> Result<Value, String> {
    let mut guard = runtime.two_factor.lock().map_err(|_| "登录状态不可用")?;
    *guard = None;
    if v["twoFactorRequired"] == true {
        let challenge = required_string(&v, "challenge")?;
        if challenge.len() != 43 {
            return Err("登录响应格式无效".into());
        }
        *guard = Some(Zeroizing::new(challenge));
        return Ok(json!({"twoFactorRequired":true}));
    }
    c.finish_login(v)?;
    c.refresh_vault()?;
    Ok(json!({}))
}
fn refresh_account(c: &mut Connection) -> Result<(), String> {
    let v = c.request(Method::GET, "/v1/account", None)?;
    c.disk.account = Some(serde_json::from_value(v).map_err(|_| "账号资料格式无效")?);
    c.disk.save(&c.root)
}
fn save_account_reply(c: &mut Connection, v: Value) -> Result<(), String> {
    c.disk.account = Some(serde_json::from_value(v).map_err(|_| "账号资料格式无效")?);
    c.disk.save(&c.root)
}
fn security_action(
    c: &mut Connection,
    path: &str,
    password: String,
    code: String,
) -> Result<Value, String> {
    let password = Zeroizing::new(password);
    let code = Zeroizing::new(code);
    c.request(
        Method::POST,
        path,
        Some(&json!({"currentPassword":password.as_str(),"code":code.as_str()})),
    )
}
fn retain_two_factor_codes(
    runtime: &Runtime,
    account: &str,
    mut response: Value,
    persist: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Value, String> {
    let encoded = Zeroizing::new(response.to_string());
    *runtime.backup_codes.lock().map_err(|_| "登录状态不可用")? =
        Some((account.into(), encoded.clone()));
    response["backupCodesPersisted"] = json!(persist(&encoded).is_ok());
    Ok(response)
}
pub(super) fn pending_two_factor_codes(c: &Connection, runtime: &Runtime) -> Result<Value, String> {
    let account = c.account_id()?;
    let stored = vault::read(&format!("{}:{account}:2fa-recovery-pending", c.ns));
    if let Ok(Some(value)) = stored {
        let mut result: Value = serde_json::from_str(&value).map_err(|_| "恢复码格式无效")?;
        result["backupCodesPersisted"] = json!(true);
        return Ok(result);
    }
    if let Some((id, value)) = runtime.backup_codes.lock().map_err(|_| "登录状态不可用")?.as_ref() {
        if id == &account {
            let mut result: Value = serde_json::from_str(value).map_err(|_| "恢复码格式无效")?;
            result["backupCodesPersisted"] = json!(false);
            return Ok(result);
        }
    }
    Ok(json!({}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_export_accepts_only_native_secret_kinds_without_renderer_paths_or_content() {
        assert!(serde_json::from_value::<Action>(json!({"type":"saveRecoveryFile","kind":"vault"})).is_ok());
        assert!(serde_json::from_value::<Action>(json!({"type":"saveRecoveryFile","kind":"twoFactor"})).is_ok());
        assert!(serde_json::from_value::<Action>(json!({"type":"saveRecoveryFile","kind":"other"})).is_err());
        assert!(serde_json::from_value::<Action>(json!({"type":"saveRecoveryFile","kind":"vault","path":"arbitrary.txt"})).is_err());
        assert!(serde_json::from_value::<Action>(json!({"type":"saveRecoveryFile","kind":"vault","content":"arbitrary"})).is_err());
    }
    #[test]
    fn successful_enable_delivers_recovery_codes_even_when_credential_store_fails() {
        let runtime = Runtime::default();
        let result = retain_two_factor_codes(
            &runtime,
            "account-a",
            json!({"backupCodes":["synthetic-one-time-code"]}),
            |_| Err("unavailable".into()),
        )
        .unwrap();
        assert_eq!(result["backupCodes"][0], "synthetic-one-time-code");
        assert_eq!(result["backupCodesPersisted"], false);
        let pending = runtime.backup_codes.lock().unwrap();
        let (account, value) = pending.as_ref().unwrap();
        assert_eq!(account, "account-a");
        assert!(value.contains("synthetic-one-time-code"));
    }
}
#[tauri::command]
pub async fn cloud_status(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app.state::<OperationLock>();
        let _lock = guard.0.lock().map_err(|_| "操作锁不可用")?;
        let mut c = Connection::load(&app.state::<data_root::DataRoot>().0)?;
        if c.account_id().is_ok() {
            let _ = refresh_account(&mut c);
        }
        c.status()
    })
    .await
    .map_err(|_| "无法读取账号状态")?
}
#[tauri::command]
pub async fn cloud_action(app: tauri::AppHandle, action: Action) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app.state::<OperationLock>();
        let _lock = guard.0.lock().map_err(|_| "操作锁不可用")?;
        let root = &app.state::<data_root::DataRoot>().0;
        let mut c = Connection::load(root)?;
        let runtime = app.state::<profiles::Runtime>();
        let mut extra = json!({});
        match action {
            Action::RequestEmail { email } => {
                c.client.call(
                    Method::POST,
                    "/v1/auth/email/request",
                    None,
                    Some(&json!({"email":email})),
                )?;
            }
            Action::VerifyEmail { email, code } => {
                let v = c.client.call(
                    Method::POST,
                    "/v1/auth/email/verify",
                    None,
                    Some(&json!({"email":email,"code":code,"device":c.device.registration()})),
                )?;
                extra = finish_primary_login(&mut c, &app.state::<Runtime>(), v)?;
            }
            Action::PasswordLogin { email, password } => {
                let password = Zeroizing::new(password);
                let v = c.client.call(Method::POST, "/v1/auth/password", None,
                    Some(&json!({"email":email,"password":password.as_str(),"device":c.device.registration()})))?;
                extra = finish_primary_login(&mut c, &app.state::<Runtime>(), v)?;
            }
            Action::VerifyTwoFactor { code } => {
                let store = app.state::<Runtime>();
                let mut guard = store.two_factor.lock().map_err(|_| "登录状态不可用")?;
                let challenge = guard.as_ref().ok_or("请重新登录")?;
                let code = Zeroizing::new(code);
                let v = c.client.call(Method::POST, "/v1/auth/2fa", None,
                    Some(&json!({"challenge":challenge.as_str(),"code":code.as_str()})))?;
                *guard = None;
                c.finish_login(v)?;
                c.refresh_vault()?;
            }
            Action::CancelLogin => {
                *app.state::<Runtime>().two_factor.lock().map_err(|_| "登录状态不可用")? = None;
                *app.state::<Runtime>().oauth.lock().map_err(|_| "登录状态不可用")? = None;
            }
            Action::SetPassword { password, current_password, code } => {
                let password = Zeroizing::new(password);
                let proof = Zeroizing::new(current_password);
                let code = Zeroizing::new(code);
                let v = c.request(Method::POST, "/v1/account/password", Some(&json!({"password":password.as_str(),"currentPassword":proof.as_str(),"code":code.as_str()})))?;
                save_account_reply(&mut c, v)?;
            }
            Action::GoogleUnlink { current_password, code } => {
                let v = security_action(&mut c, "/v1/account/google/unlink", current_password, code)?;
                save_account_reply(&mut c, v)?;
            }
            Action::StartTwoFactor { current_password, code } => {
                extra = security_action(&mut c, "/v1/account/2fa/start", current_password, code)?;
            }
            Action::EnableTwoFactor { code } => {
                let code = Zeroizing::new(code);
                extra = c.request(Method::POST, "/v1/account/2fa/enable", Some(&json!({"code":code.as_str()})))?;
                // Never discard one-time codes because a later metadata request failed.
                let account = c.account_id()?;
                extra = retain_two_factor_codes(&app.state::<Runtime>(), &account, extra, |value| vault::write(&format!("{}:{account}:2fa-recovery-pending",c.ns), value))?;
                if let Some(a) = c.disk.account.as_mut() { a.two_factor_enabled = true; a.recent_auth = true; }
                let _ = c.disk.save(root);
            }
            Action::PendingTwoFactorRecovery => {
                extra = pending_two_factor_codes(&c, &app.state::<Runtime>())?;
            }
            Action::AcknowledgeTwoFactorRecovery => {
                let account = c.account_id()?;
                vault::remove(&format!("{}:{account}:2fa-recovery-pending",c.ns))?;
                *app.state::<Runtime>().backup_codes.lock().map_err(|_| "登录状态不可用")? = None;
            }
            Action::DisableTwoFactor { current_password, code } => {
                let v = security_action(&mut c, "/v1/account/2fa/disable", current_password, code)?;
                save_account_reply(&mut c, v)?;
                let account = c.account_id()?;
                let _ = vault::remove(&format!("{}:{account}:2fa-recovery-pending",c.ns));
                *app.state::<Runtime>().backup_codes.lock().map_err(|_| "登录状态不可用")? = None;
            }
            Action::GoogleStart => {
                let v = c.client.call(
                    Method::POST,
                    "/v1/auth/google/start",
                    None,
                    Some(&json!({"device":c.device.registration()})),
                )?;
                let url = required_string(&v, "url")?;
                let parsed = reqwest::Url::parse(&url).map_err(|_| "Google 登录地址无效")?;
                if parsed.scheme() != "https"
                    || parsed.host_str() != Some("accounts.google.com")
                    || !parsed.username().is_empty()
                {
                    return Err("Google 登录地址校验失败".into());
                };
                let poll = Zeroizing::new(required_string(&v, "pollToken")?);
                *app.state::<Runtime>()
                    .oauth
                    .lock()
                    .map_err(|_| "登录状态不可用")? = Some(poll);
                platform::open_external(&url)?;
                extra = json!({"pending":true});
            }
            Action::GooglePoll => {
                let store = app.state::<Runtime>();
                let mut guard = store.oauth.lock().map_err(|_| "登录状态不可用")?;
                let poll = guard.as_ref().ok_or("Google 登录已结束，请重新开始")?;
                let body = json!({"pollToken":poll.as_str()});
                let v = if c.account_id().is_ok() {
                    c.request(Method::POST, "/v1/auth/google/poll", Some(&body))?
                } else {
                    c.client
                        .call(Method::POST, "/v1/auth/google/poll", None, Some(&body))?
                };
                if v["pending"] == true {
                    extra = json!({"pending":true})
                } else {
                    if v["linked"] != true {
                        c.finish_login(v)?
                    };
                    c.refresh_vault()?;
                    *guard = None;
                    *store.two_factor.lock().map_err(|_| "登录状态不可用")? = None;
                }
            }
            Action::ConfirmDevice { code } => c.confirm_device(&code)?,
            Action::Refresh => {
                c.refresh_vault()?;
            }
            Action::Initialize => extra = c.initialize()?,
            Action::PendingRecovery => extra = c.pending_recovery()?,
            Action::SaveRecoveryFile { kind } => {
                let window = app.get_webview_window("main").ok_or("应用窗口不可用")?;
                extra = super::recovery_export::save(&window, &mut c, &app.state::<Runtime>(), kind)?;
            }
            Action::AcknowledgeRecovery => c.acknowledge_recovery()?,
            Action::Recover { code } => c.recover(&code)?,
            Action::UpdateAccount { name, avatar } => {
                let v = c.request(
                    Method::PATCH,
                    "/v1/account",
                    Some(&json!({"name":name,"avatar":avatar})),
                )?;
                c.disk.account = Some(serde_json::from_value(v).map_err(|_| "账号资料格式无效")?);
                c.disk.save(root)?;
            }
            Action::Logout => {
                *app.state::<Runtime>().backup_codes.lock().map_err(|_| "登录状态不可用")? = None;
                *app.state::<Runtime>().two_factor.lock().map_err(|_| "登录状态不可用")? = None;
                *app.state::<Runtime>()
                    .oauth
                    .lock()
                    .map_err(|_| "登录状态不可用")? = None;
                c.logout()?;
            }
            Action::Devices => extra = c.request(Method::GET, "/v1/devices", None)?,
            Action::ApproveDevice {
                id,
                verification_code,
            } => c.approve(&id, &verification_code)?,
            Action::RevokeDevice { id } => extra = super::rotation::revoke(&mut c, &id)?,
            Action::Preview => extra = sync::preview(&mut c)?,
            Action::Setup {
                mode,
                local_ids,
                cloud_ids,
                sessions,
            } => sync::setup(&mut c, &mode, &local_ids, &cloud_ids, sessions, &runtime)?,
            Action::Sync => match sync::run(&mut c, &runtime) {
                Ok(()) => {}
                Err(e) => {
                    if let Ok(id) = c.account_id() {
                        c.disk.accounts.get_mut(&id).unwrap().last_error = Some(e.clone());
                        c.disk.save(root)?
                    };
                    return Err(e);
                }
            },
            Action::Preferences {
                auto_sync,
                session_sync,
                pull_cloud,
            } => {
                let id = c.account_id()?;
                // Saving preferences does not run sync; scheduler/run retain all setup and key guards.
                let s = c.disk.accounts.get_mut(&id).unwrap();
                s.auto_sync = auto_sync;
                s.session_sync = session_sync;
                s.pull_cloud = pull_cloud;
                c.disk.save(root)?;
            }
            Action::Resolve { id, kind, choice } => match kind.as_str() {
                "config" => sync::resolve(&mut c, &id, &choice, &runtime)?,
                "session" => super::session_sync::resolve(&mut c, &id, &choice, &runtime)?,
                _ => return Err("冲突类型无效".into()),
            },
            Action::Detach { id, remove_local } => {
                sync::detach(&mut c, &id, remove_local, &runtime)?
            }
        }
        let status = if extra["backupCodes"].is_array() {
            c.status().unwrap_or_else(|_| json!({"account":c.disk.account}))
        } else { c.status()? };
        let _ = app.emit("cloud-changed", ());
        let _ = app.emit("profiles-changed", ());
        Ok(json!({"status":status,"result":extra}))
    })
    .await
    .map_err(|_| "账号操作未完成")?
}
