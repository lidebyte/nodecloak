//! OS credential store only. There is intentionally no file fallback.
use super::crypto::{self, Key};
use base64::{
    engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as URL64},
    Engine,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};
pub const SERVICE: &str = "com.nodecloak.cloud";
pub fn namespace(endpoint: &str) -> String {
    format!("{:x}", Sha256::digest(endpoint.as_bytes()))
}
pub fn read(name: &str) -> Result<Option<Zeroizing<String>>, String> {
    match keyring::Entry::new(SERVICE, name)
        .map_err(|_| "系统密钥库不可用")?
        .get_password()
    {
        Ok(s) => Ok(Some(Zeroizing::new(s))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("无法读取系统密钥库".into()),
    }
}
pub fn write(name: &str, value: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, name)
        .map_err(|_| "系统密钥库不可用")?
        .set_password(value)
        .map_err(|_| "无法保存系统密钥库".into())
}
pub fn remove(name: &str) -> Result<(), String> {
    match keyring::Entry::new(SERVICE, name)
        .map_err(|_| "系统密钥库不可用")?
        .delete_credential()
    {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法清除系统密钥库".into()),
    }
}
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub private_key: String,
    pub secret: String,
}
impl Device {
    pub fn load(ns: &str) -> Result<Self, String> {
        let name = format!("{ns}:device");
        if let Some(s) = read(&name)? {
            let d: Self = serde_json::from_str(&s).map_err(|_| "设备密钥格式无效")?;
            if uuid::Uuid::parse_str(&d.id).is_err()
                || decode_key(&d.private_key).is_err()
                || decode_key(&d.public_key).is_err()
                || decode_key(&d.secret).is_err()
            {
                return Err("设备密钥格式无效".into());
            };
            return Ok(d);
        }
        let keys = crypto::DeviceKeys::generate()?;
        let secret = crypto::random_key()?;
        let d = Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| std::env::consts::OS.into()),
            public_key: B64.encode(keys.public_key),
            private_key: B64.encode(keys.private_key.as_slice()),
            secret: B64.encode(secret.as_slice()),
        };
        write(
            &name,
            &Zeroizing::new(serde_json::to_string(&d).map_err(|_| "设备密钥格式无效")?),
        )?;
        Ok(d)
    }
    pub fn registration(&self) -> serde_json::Value {
        serde_json::json!({"id":self.id,"name":self.name,"publicKey":self.public_key,"secret":self.secret})
    }
}
pub fn decode_key(s: &str) -> Result<Key, String> {
    let bytes = Zeroizing::new(B64.decode(s).map_err(|_| "密钥格式无效")?);
    Ok(Zeroizing::new(
        bytes.as_slice().try_into().map_err(|_| "密钥格式无效")?,
    ))
}
pub fn root_name(ns: &str, account: &str, epoch: u32) -> String {
    format!("{ns}:{account}:root:{epoch}")
}
pub fn root(ns: &str, account: &str, epoch: u32) -> Result<Option<Key>, String> {
    read(&root_name(ns, account, epoch))?
        .map(|s| decode_key(&s))
        .transpose()
}
pub fn staged_root(ns: &str, account: &str, epoch: u32) -> Result<Option<Key>, String> {
    read(&format!("{ns}:{account}:root-pending:{epoch}"))?
        .map(|s| decode_key(&s))
        .transpose()
}
pub fn stage_root(ns: &str, account: &str, epoch: u32, key: &[u8; 32]) -> Result<(), String> {
    write(
        &format!("{ns}:{account}:root-pending:{epoch}"),
        &Zeroizing::new(B64.encode(key)),
    )
}
pub fn clear_staged(ns: &str, account: &str, epoch: u32) -> Result<(), String> {
    remove(&format!("{ns}:{account}:root-pending:{epoch}"))
}
pub fn save_root(ns: &str, account: &str, epoch: u32, key: &[u8; 32]) -> Result<(), String> {
    write(
        &root_name(ns, account, epoch),
        &Zeroizing::new(B64.encode(key)),
    )
}
pub fn recovery_code(key: &[u8; 32]) -> Zeroizing<String> {
    Zeroizing::new(format!("nc1_{}", URL64.encode(key)))
}
pub fn parse_recovery(s: &str) -> Result<Key, String> {
    let encoded = s.trim().strip_prefix("nc1_").ok_or("恢复密钥格式无效")?;
    let bytes = Zeroizing::new(URL64.decode(encoded).map_err(|_| "恢复密钥格式无效")?);
    Ok(Zeroizing::new(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| "恢复密钥格式无效")?,
    ))
}
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct PendingRecovery {
    pub epoch: u32,
    pub code: String,
}
pub fn pending(ns: &str, account: &str) -> Result<Option<PendingRecovery>, String> {
    read(&format!("{ns}:{account}:recovery-pending"))?
        .map(|v| serde_json::from_str(&v).map_err(|_| "待确认恢复密钥损坏".into()))
        .transpose()
}
pub fn save_pending(ns: &str, account: &str, epoch: u32, secret: &[u8; 32]) -> Result<(), String> {
    let p = PendingRecovery {
        epoch,
        code: recovery_code(secret).to_string(),
    };
    write(
        &format!("{ns}:{account}:recovery-pending"),
        &Zeroizing::new(serde_json::to_string(&p).map_err(|_| "无法保存恢复密钥")?),
    )
}
