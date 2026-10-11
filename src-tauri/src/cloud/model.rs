use crate::{engine, profiles, proxy::ProxyConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PortableProxy {
    pub mode: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Document {
    pub schema: u8,
    pub name: String,
    pub browser: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub proxy: PortableProxy,
    pub preferences: profiles::Preferences,
    pub deleted: bool,
}
impl Document {
    pub fn export(p: &profiles::Profile) -> Result<Self, String> {
        let upstream = profiles::upstream(p, false)?;
        let mut prefs = p.preferences.clone();
        crate::regional::normalize_sources(&mut prefs);
        Ok(Self {
            schema: 1,
            name: p.name.clone(),
            browser: p.browser.clone(),
            notes: p.notes.clone(),
            tags: p.tags.clone(),
            proxy: PortableProxy {
                mode: p.proxy.mode.clone(),
                host: p.proxy.host.clone(),
                port: p.proxy.port,
                username: p.proxy.username.clone(),
                password: upstream.password.clone(),
            },
            preferences: prefs,
            deleted: p.deleted_at.is_some(),
        })
    }
    pub fn draft(&self) -> profiles::Draft {
        profiles::Draft {
            name: self.name.clone(),
            browser: self.browser.clone(),
            notes: self.notes.clone(),
            tags: self.tags.clone(),
            proxy: ProxyConfig {
                mode: self.proxy.mode.clone(),
                host: self.proxy.host.clone(),
                port: self.proxy.port,
                username: self.proxy.username.clone(),
                credential_ref: None,
            },
            password: Some(self.proxy.password.clone()),
            clear_password: self.proxy.password.is_empty(),
            preferences: self.preferences.clone(),
        }
    }
    pub fn parse(v: Value) -> Result<Self, String> {
        let mut doc: Self = serde_json::from_value(v).map_err(|_| "云端副本格式无效")?;
        if doc.schema != 1 {
            return Err("不支持的云端副本版本".into());
        };
        crate::regional::normalize_sources(&mut doc.preferences);
        profiles::validate(&doc.draft())?;
        Ok(doc)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar: String,
    #[serde(default)]
    pub google_linked: bool,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub two_factor_enabled: bool,
    #[serde(default)]
    pub recent_auth: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Object {
    pub id: String,
    pub kind: String,
    pub revision: u64,
    pub epoch: u32,
    pub deleted: bool,
    #[serde(default)]
    pub payload: String,
    pub writer: String,
    pub generation: u64,
    pub updated_at: String,
}
impl Object {
    pub fn context<'a>(&'a self, account: &'a str) -> super::crypto::Context<'a> {
        super::crypto::Context {
            account,
            object: &self.id,
            kind: &self.kind,
            revision: self.revision,
            epoch: self.epoch,
            deleted: self.deleted,
            generation: self.generation,
            writer: &self.writer,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub cloud_id: String,
    pub baseline: Option<Object>,
    pub session_baseline: Option<Object>,
    #[serde(default)]
    pub session_checkpoint: Option<Object>,
    pub generation: u64,
    pub session_dirty: bool,
    pub pending_session: Option<Object>,
    pub pending_config: Option<Object>,
    #[serde(default)]
    pub session_hash: Option<String>,
    #[serde(default)]
    pub file_stamp: Option<String>,
}
impl Binding {
    pub fn new(id: &str) -> Self {
        Self {
            cloud_id: id.into(),
            baseline: None,
            session_baseline: None,
            session_checkpoint: None,
            generation: 0,
            session_dirty: false,
            pending_session: None,
            pending_config: None,
            session_hash: None,
            file_stamp: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conflict {
    pub local_id: String,
    pub cloud_id: String,
    pub name: String,
    pub kind: String,
    pub fields: Vec<String>,
    pub remote_revision: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountState {
    pub auto_sync: bool,
    pub session_sync: bool,
    pub setup_complete: bool,
    pub recovery_saved: bool,
    pub epoch: u32,
    pub authorized_device: Option<String>,
    pub pull_cloud: bool,
    pub bindings: BTreeMap<String, Binding>,
    pub excluded: BTreeSet<String>,
    pub detached: BTreeSet<String>,
    pub known_local: BTreeSet<String>,
    pub conflicts: Vec<Conflict>,
    pub last_sync: Option<String>,
    pub last_error: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Disk {
    pub account: Option<Account>,
    pub accounts: BTreeMap<String, AccountState>,
}
impl Disk {
    pub fn load(root: &Path) -> Result<Self, String> {
        let file = root.join("cloud/state.json");
        profiles::guard_tree(&file)?;
        match std::fs::read(&file) {
            Ok(b) => {
                if b.len() > 24 << 20 {
                    return Err("云端状态文件过大".into());
                };
                let disk: Self = serde_json::from_slice(&b).map_err(|_| "云端状态文件损坏")?;
                for (a, s) in &disk.accounts {
                    if uuid::Uuid::parse_str(a).is_err()
                        || s.bindings
                            .values()
                            .any(|b| uuid::Uuid::parse_str(&b.cloud_id).is_err())
                    {
                        return Err("云端标识无效".into());
                    }
                }
                Ok(disk)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("无法读取云端状态".into()),
        }
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        let file = root.join("cloud/state.json");
        profiles::guard_tree(&file)?;
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|_| "无法创建云端状态目录")?;
        engine::atomic_write(
            &file,
            &serde_json::to_vec(self).map_err(|_| "无法序列化云端状态")?,
        )
    }
    pub fn can_attach(&self, account: &str, local_id: &str) -> Result<(), String> {
        if self
            .accounts
            .iter()
            .any(|(id, s)| id != account && s.bindings.contains_key(local_id))
        {
            Err("此副本关联了其他账号，请先复制副本再关联".into())
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_profile_projection_excludes_device_paths_and_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::profiles::load(dir.path()).unwrap().remove(0);
        let doc = Document::export(&p).unwrap();
        let value = serde_json::to_value(doc).unwrap();
        assert_eq!(value["browser"], "chrome");
        assert!(value.get("id").is_none());
        for key in [
            "relayPort",
            "activeProxy",
            "lastTest",
            "legacy",
            "credentialRefs",
            "lastRegion",
            "createdAt",
            "updatedAt",
        ] {
            assert!(value.get(key).is_none(), "{key}")
        }
        assert!(value["proxy"].get("credentialRef").is_none());
    }
    #[test]
    fn imported_profile_rejects_unknown_schema_and_platform_state() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::profiles::load(dir.path()).unwrap().remove(0);
        let doc = Document::export(&p).unwrap();
        let mut v = serde_json::to_value(doc).unwrap();
        v["schema"] = serde_json::json!(2);
        assert!(Document::parse(v).is_err());
        let doc = Document::export(&p).unwrap();
        let mut v = serde_json::to_value(doc).unwrap();
        v["relayPort"] = serde_json::json!(1234);
        assert!(Document::parse(v).is_err());
    }
    #[test]
    fn new_account_cannot_implicitly_claim_previous_account_profiles() {
        let mut state = Disk::default();
        state
            .accounts
            .entry("first".into())
            .or_default()
            .bindings
            .insert("legacy-chrome".into(), Binding::new("cloud-a"));
        assert!(state.can_attach("second", "legacy-chrome").is_err());
        assert!(state.can_attach("first", "legacy-chrome").is_ok());
    }
}
