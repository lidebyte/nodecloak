use super::{
    client::Client,
    crypto,
    model::{Account, Disk},
    vault,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub account_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}
pub struct Connection {
    pub client: Client,
    pub ns: String,
    pub device: vault::Device,
    pub disk: Disk,
    pub root: PathBuf,
    session: Option<Session>,
}
impl Connection {
    #[cfg(test)]
    pub fn test_connection(root: &Path, client: Client, ns: &str) -> Result<Self, String> {
        Ok(Self {
            client,
            ns: ns.into(),
            device: vault::Device::load(ns)?,
            disk: Disk::load(root)?,
            root: root.into(),
            session: None,
        })
    }
    pub fn load(root: &Path) -> Result<Self, String> {
        let client = Client::configured()?;
        Self::with_client(root, client)
    }
    pub fn with_client(root: &Path, client: Client) -> Result<Self, String> {
        let ns = vault::namespace(&client.endpoint);
        let device = vault::Device::load(&ns)?;
        let disk = Disk::load(root)?;
        let session = vault::read(&format!("{ns}:session"))?
            .map(|s| serde_json::from_str::<Session>(&s).map_err(|_| "登录凭据损坏，请重新登录"))
            .transpose()?;
        Ok(Self {
            client,
            ns,
            device,
            disk,
            root: root.into(),
            session,
        })
    }
    pub fn save_session(&mut self, s: Session) -> Result<(), String> {
        vault::write(
            &format!("{}:session", self.ns),
            &Zeroizing::new(serde_json::to_string(&s).map_err(|_| "无法保存登录凭据")?),
        )?;
        self.session = Some(s);
        Ok(())
    }
    pub fn account_id(&self) -> Result<String, String> {
        let s = self.session.as_ref().ok_or("请先登录")?;
        if self.disk.account.as_ref().map(|a| a.id.as_str()) != Some(s.account_id.as_str()) {
            return Err("账号状态不一致，请重新登录".into());
        };
        Ok(s.account_id.clone())
    }
    pub fn request(
        &mut self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        self.account_id()?;
        if self.session.as_ref().unwrap().expires_at <= chrono::Utc::now().timestamp() + 30 {
            self.refresh_session()?
        }
        let result = self.client.call(
            method.clone(),
            path,
            Some(&self.session.as_ref().unwrap().access_token),
            body,
        );
        match result {
            Err(e) if e.contains("unauthorized") => {
                self.refresh_session()?;
                self.client.call(
                    method,
                    path,
                    Some(&self.session.as_ref().unwrap().access_token),
                    body,
                )
            }
            other => other,
        }
    }
    fn refresh_session(&mut self) -> Result<(), String> {
        let old = self.session.as_ref().ok_or("请先登录")?;
        let result = self.client.call(
            Method::POST,
            "/v1/auth/refresh",
            None,
            Some(&json!({"refreshToken":old.refresh_token})),
        );
        let value = match result {
            Err(e) if e.contains("unauthorized") => {
                vault::remove(&format!("{}:session", self.ns))?;
                self.session = None;
                self.disk.account = None;
                self.disk.save(&self.root)?;
                return Err("unauthorized: 请重新登录".into());
            }
            v => v?,
        };
        let old = self.session.as_ref().unwrap();
        let s = Session {
            account_id: old.account_id.clone(),
            access_token: required_string(&value, "accessToken")?,
            refresh_token: required_string(&value, "refreshToken")?,
            expires_at: chrono::Utc::now().timestamp() + value["expiresIn"].as_i64().unwrap_or(900),
        };
        self.save_session(s)
    }
    pub fn finish_login(&mut self, v: Value) -> Result<(), String> {
        let account: Account =
            serde_json::from_value(v["account"].clone()).map_err(|_| "账号响应格式无效")?;
        if uuid::Uuid::parse_str(&account.id).is_err()
            || v["deviceId"].as_str() != Some(self.device.id.as_str())
        {
            return Err("账号设备标识无效".into());
        };
        let s = Session {
            account_id: account.id.clone(),
            access_token: required_string(&v, "accessToken")?,
            refresh_token: required_string(&v, "refreshToken")?,
            expires_at: chrono::Utc::now().timestamp() + 900,
        };
        self.save_session(s)?;
        self.disk.accounts.entry(account.id.clone()).or_default();
        self.disk.account = Some(account);
        self.disk.save(&self.root)
    }
    pub fn refresh_vault(&mut self) -> Result<Value, String> {
        let id = self.account_id()?;
        let account = self.request(Method::GET, "/v1/account", None)?;
        self.disk.account = Some(serde_json::from_value(account).map_err(|_| "账号响应格式无效")?);
        let info = self.request(Method::GET, "/v1/vault", None)?;
        let epoch = info["epoch"].as_u64().unwrap_or(0) as u32;
        let pending = vault::pending(&self.ns, &id)?;
        let mut pending_current = false;
        let mut previously_trusted = false;
        if epoch < self.disk.accounts[&id].epoch {
            return Err("云端密钥版本倒退，请停止同步".into());
        };
        if info["approved"] == true && epoch > 0 {
            let env = required_string(&info, "deviceEnvelope")?;
            let key = crypto::device_open(
                &*vault::decode_key(&self.device.private_key)?,
                &id,
                &self.device.id,
                epoch,
                &env,
            )?;
            if let Some(existing) = vault::root(&self.ns, &id, epoch)? {
                if *existing != *key {
                    return Err("云端密钥与本机记录不一致，请停止同步并检查设备授权".into());
                };
                previously_trusted = true;
            }
            pending_current = pending.as_ref().is_some_and(|p| {
                confirmed_pending(
                    p,
                    &id,
                    epoch,
                    info["recoveryEnvelope"].as_str().unwrap_or(""),
                    &key,
                )
            });
            if !crypto::verify_device_proof(
                &key,
                &id,
                &self.device.id,
                &self.device.public_key,
                epoch,
                info["deviceProof"].as_str().unwrap_or(""),
            ) {
                return Err("设备授权记录校验失败".into());
            };
            if previously_trusted || pending_current {
                vault::save_root(&self.ns, &id, epoch, &key)?;
                vault::clear_staged(&self.ns, &id, epoch)?;
            } else {
                vault::stage_root(&self.ns, &id, epoch, &key)?;
            }
        }
        if pending.as_ref().is_some_and(|p| {
            p.epoch < epoch || (p.epoch == epoch && info["approved"] == true && !pending_current)
        }) {
            vault::remove(&format!("{}:{id}:recovery-pending", self.ns))?;
        }
        let state = self.disk.accounts.entry(id).or_default();
        if info["approved"] == true {
            state.recovery_saved = !pending_current;
        }
        state.epoch = epoch;
        state.authorized_device =
            if info["approved"] == true && (previously_trusted || pending_current) {
                Some(self.device.id.clone())
            } else {
                None
            };
        self.disk.save(&self.root)?;
        Ok(info)
    }
    pub fn confirm_device(&mut self, code: &str) -> Result<(), String> {
        let info = self.refresh_vault()?;
        if info["approved"] != true {
            return Err("本设备尚未获授权".into());
        }
        let id = self.account_id()?;
        let epoch = self.disk.accounts[&id].epoch;
        let root = vault::staged_root(&self.ns, &id, epoch)?
            .or(vault::root(&self.ns, &id, epoch)?)
            .ok_or("本设备尚未获授权")?;
        if code.replace(' ', "").to_lowercase() != crypto::vault_code(&id, &root) {
            return Err("加密空间校验码不匹配".into());
        };
        vault::save_root(&self.ns, &id, epoch, &root)?;
        vault::clear_staged(&self.ns, &id, epoch)?;
        self.disk.accounts.get_mut(&id).unwrap().authorized_device = Some(self.device.id.clone());
        self.disk.save(&self.root)
    }
    pub fn unlocked_root(&self) -> Result<crypto::Key, String> {
        let id = self.account_id()?;
        let state = self.disk.accounts.get(&id).ok_or("云端状态不存在")?;
        if state.authorized_device.as_deref() != Some(self.device.id.as_str()) {
            return Err("请先授权本设备或输入恢复密钥".into());
        };
        vault::root(&self.ns, &id, state.epoch)?
            .ok_or_else(|| "请先授权本设备或输入恢复密钥".into())
    }
    pub fn status(&self) -> Result<Value, String> {
        let Some(s) = &self.session else {
            return Ok(json!({"account":null,"endpoint":self.client.endpoint}));
        };
        let Some(a) = &self.disk.account else {
            return Ok(json!({"account":null,"endpoint":self.client.endpoint}));
        };
        if a.id != s.account_id {
            return Ok(json!({"account":null,"endpoint":self.client.endpoint}));
        }
        let state = self.disk.accounts.get(&a.id).ok_or("云端状态不存在")?;
        let fp = fingerprint(&a.id, &self.device.id, &self.device.public_key);
        Ok(
            json!({"account":a,"endpoint":self.client.endpoint,"deviceId":self.device.id,"deviceName":self.device.name,"fingerprint":fp,"vaultCode":if state.authorized_device.as_deref()==Some(self.device.id.as_str()){vault::root(&self.ns,&a.id,state.epoch)?.map(|k|crypto::vault_code(&a.id,&k))}else{None},"approvedNeedsVerification":state.epoch>0&&state.authorized_device.is_none()&&vault::staged_root(&self.ns,&a.id,state.epoch)?.is_some(),"initialized":state.epoch>0,"unlocked":state.authorized_device.as_deref()==Some(self.device.id.as_str())&&vault::root(&self.ns,&a.id,state.epoch)?.is_some(),"recoverySaved":state.recovery_saved,"setupComplete":state.setup_complete,"autoSync":state.auto_sync,"sessionSync":state.session_sync,"pullCloud":state.pull_cloud,"lastSync":state.last_sync,"lastError":state.last_error,"conflicts":state.conflicts,"bindings":state.bindings.iter().filter(|(id,_)|!state.detached.contains(*id)).map(|(local,b)|json!({"localId":local,"cloudId":b.cloud_id,"sessionPending":b.pending_session.is_some(),"configPending":b.pending_config.is_some()})).collect::<Vec<_>>()}),
        )
    }
    pub fn initialize(&mut self) -> Result<Value, String> {
        let info = self.refresh_vault()?;
        if info["initialized"] == true {
            return Err("云端已初始化，请授权设备或使用恢复密钥".into());
        };
        let id = self.account_id()?;
        let root = crypto::random_key()?;
        let recovery = crypto::random_key()?;
        let body = json!({"deviceProof":crypto::device_proof(&root,&id,&self.device.id,&self.device.public_key,1),"epoch":1,"recoveryEnvelope":crypto::recovery_seal(&recovery,&id,1,&root)?,"recoveryVerifier":B64.encode(crypto::recovery_verifier(&recovery)?),"deviceEnvelope":crypto::device_seal(&*vault::decode_key(&self.device.public_key)?,&id,&self.device.id,1,&root)?});
        // Persist pending secrets first so a network disconnect cannot strand an initialized vault.
        vault::stage_root(&self.ns, &id, 1, &root)?;
        vault::save_pending(&self.ns, &id, 1, &recovery)?;
        self.request(Method::POST, "/v1/vault/initialize", Some(&body))?;
        vault::save_root(&self.ns, &id, 1, &root)?;
        vault::clear_staged(&self.ns, &id, 1)?;
        let state = self.disk.accounts.get_mut(&id).unwrap();
        state.epoch = 1;
        state.recovery_saved = false;
        state.authorized_device = Some(self.device.id.clone());
        self.disk.save(&self.root)?;
        Ok(json!({"recoveryCode":vault::recovery_code(&recovery).as_str()}))
    }
    pub fn acknowledge_recovery(&mut self) -> Result<(), String> {
        self.pending_recovery()?;
        let id = self.account_id()?;
        vault::remove(&format!("{}:{id}:recovery-pending", self.ns))?;
        self.disk.accounts.get_mut(&id).unwrap().recovery_saved = true;
        self.disk.save(&self.root)
    }
    pub fn pending_recovery(&mut self) -> Result<Value, String> {
        let info = self.refresh_vault()?;
        let id = self.account_id()?;
        let pending = vault::pending(&self.ns, &id)?.ok_or("恢复密钥已经确认保存")?;
        let epoch = self.disk.accounts[&id].epoch;
        let root = self.unlocked_root()?;
        if !confirmed_pending(
            &pending,
            &id,
            epoch,
            info["recoveryEnvelope"].as_str().unwrap_or(""),
            &root,
        ) {
            return Err("请先联网确认密钥轮换结果".into());
        };
        Ok(json!({"recoveryCode":pending.code}))
    }
    pub fn recover(&mut self, code: &str) -> Result<(), String> {
        let secret = vault::parse_recovery(code)?;
        // A recovery secret authenticates the root independently of device metadata.
        let info = self.request(Method::GET, "/v1/vault", None)?;
        let id = self.account_id()?;
        let epoch = info["epoch"].as_u64().ok_or("云端尚未初始化")? as u32;
        if epoch < self.disk.accounts[&id].epoch {
            return Err("云端密钥版本倒退，请停止同步".into());
        }
        let root = crypto::recovery_open(
            &secret,
            &id,
            epoch,
            &required_string(&info, "recoveryEnvelope")?,
        )?;
        let challenge = self.request(Method::POST, "/v1/vault/recover/start", Some(&json!({})))?;
        let challenge_token = required_string(&challenge, "challenge")?;
        let expected = format!(
            "nodecloak-recover-v1:{id}:{}:{epoch}:{challenge_token}",
            self.device.id
        );
        if challenge["message"] != expected || challenge["epoch"] != epoch {
            return Err("恢复校验信息不匹配".into());
        }
        let body = json!({"deviceProof":crypto::device_proof(&root,&id,&self.device.id,&self.device.public_key,epoch),"epoch":epoch,"envelope":crypto::device_seal(&*vault::decode_key(&self.device.public_key)?,&id,&self.device.id,epoch,&root)?,"challenge":challenge_token,"signature":B64.encode(crypto::recovery_sign(&secret,expected.as_bytes())?)});
        self.request(Method::POST, "/v1/vault/recover", Some(&body))?;
        vault::save_root(&self.ns, &id, epoch, &root)?;
        vault::clear_staged(&self.ns, &id, epoch)?;
        let state = self.disk.accounts.get_mut(&id).unwrap();
        state.epoch = epoch;
        state.recovery_saved = true;
        state.authorized_device = Some(self.device.id.clone());
        self.disk.save(&self.root)
    }
    pub fn approve(&mut self, id: &str, code: &str) -> Result<(), String> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("设备标识无效".into());
        };
        let account = self.account_id()?;
        let root = self.unlocked_root()?;
        let devices = self.request(Method::GET, "/v1/devices", None)?;
        let list = devices["devices"].as_array().ok_or("设备列表格式无效")?;
        let device = list
            .iter()
            .find(|d| d["id"] == id && d["status"] == "pending")
            .ok_or("设备请求已变化")?;
        let public = required_string(device, "publicKey")?;
        let fp = fingerprint(&account, id, &public);
        if device["fingerprint"] != fp
            || code.replace(' ', "").to_lowercase() != fp[fp.len() - 12..]
        {
            return Err("校验码不匹配，请核对新设备显示的校验码".into());
        }
        let epoch = self.disk.accounts[&account].epoch;
        let env = crypto::device_seal(&*vault::decode_key(&public)?, &account, id, epoch, &root)?;
        self.request(
            Method::POST,
            &format!("/v1/devices/{id}/approve"),
            Some(&json!({"fingerprint":fp,"epoch":epoch,"envelope":env,"deviceProof":crypto::device_proof(&root,&account,id,&public,epoch)})),
        )?;
        Ok(())
    }
    pub fn logout(&mut self) -> Result<(), String> {
        let error = self
            .request(Method::POST, "/v1/auth/logout", Some(&json!({})))
            .err();
        vault::remove(&format!("{}:session", self.ns))?;
        self.session = None;
        self.disk.account = None;
        self.disk.save(&self.root)?;
        if error.is_some() {
            return Err("本机已退出；网络不可用，服务端会话尚未撤销".into());
        };
        Ok(())
    }
}
fn confirmed_pending(
    p: &vault::PendingRecovery,
    account: &str,
    epoch: u32,
    envelope: &str,
    root: &[u8; 32],
) -> bool {
    p.epoch == epoch
        && vault::parse_recovery(&p.code)
            .and_then(|s| crypto::recovery_open(&s, account, epoch, envelope))
            .is_ok_and(|k| *k == *root)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_proposal_must_match_confirmed_root_not_just_epoch() {
        let winner = crypto::random_key().unwrap();
        let loser = crypto::random_key().unwrap();
        let root = crypto::random_key().unwrap();
        let env = crypto::recovery_seal(&winner, "account", 2, &root).unwrap();
        let p = vault::PendingRecovery {
            epoch: 2,
            code: vault::recovery_code(&loser).to_string(),
        };
        assert!(!confirmed_pending(&p, "account", 2, &env, &root));
        let p = vault::PendingRecovery {
            epoch: 2,
            code: vault::recovery_code(&winner).to_string(),
        };
        assert!(confirmed_pending(&p, "account", 2, &env, &root));
        assert!(!confirmed_pending(&p, "account", 3, &env, &root));
    }
}
pub fn fingerprint(account: &str, device: &str, public: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(format!("nodecloak-device-v1:{account}:{device}:{public}").as_bytes())
    )
}
pub fn required_string(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "云端响应缺少必要字段".into())
}
