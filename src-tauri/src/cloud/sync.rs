use super::{
    account::Connection,
    crypto, merge,
    model::{Binding, Conflict, Document, Object},
};
use crate::{platform, profiles};
use reqwest::Method;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub enum Decision {
    Unchanged,
    Upload(Value),
    Apply(Value),
    Conflict(Vec<String>),
}
pub(crate) fn ensure_purge_synced(root: &std::path::Path, id: &str) -> Result<(), String> {
    let disk = super::model::Disk::load(root)?;
    for state in disk.accounts.values() {
        if state.detached.contains(id) { continue; }
        if let Some(binding) = state.bindings.get(id) {
            if !binding.baseline.as_ref().is_some_and(|o| o.deleted) {
                return Err("副本的删除尚未同步到云端，本机数据已保留。请在管理同步中完成同步，或取消云端关联后再彻底删除。".into());
            }
        }
    }
    Ok(())
}
/// Selected MCP previews pin remote identities; successful writes advance only their own pin.
pub(crate) struct VersionGuard {
    pub ids: Vec<String>,
    pub objects: Vec<Object>,
}
impl VersionGuard {
    pub fn check(&self, id: &str, kind: &str, actual: Option<&Object>) -> Result<(), String> {
        if !self.ids.iter().any(|x| x == id) {
            return Ok(());
        }
        let expected = self.objects.iter().find(|o| o.id == id && o.kind == kind);
        let equal = match (expected, actual) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.id == b.id
                    && a.kind == b.kind
                    && a.revision == b.revision
                    && a.epoch == b.epoch
                    && a.writer == b.writer
                    && a.generation == b.generation
                    && a.deleted == b.deleted
            }
            _ => false,
        };
        if equal {
            Ok(())
        } else {
            Err("SYNC_PREVIEW_CHANGED".into())
        }
    }
    pub fn manifest(&self, list: &[Object]) -> Result<(), String> {
        for id in &self.ids {
            for kind in ["config", "session"] {
                self.check(
                    id,
                    kind,
                    list.iter().find(|o| &o.id == id && o.kind == kind),
                )?;
            }
        }
        Ok(())
    }
    fn record(&mut self, o: &Object) {
        if self.ids.contains(&o.id) {
            self.objects.retain(|x| x.id != o.id || x.kind != o.kind);
            self.objects.push(o.clone());
        }
    }
}
pub fn decide(base: Option<&Value>, local: Option<&Value>, remote: Option<&Value>) -> Decision {
    // A different data root, incomplete registry or failed migration is not a
    // user deletion. Only an explicit tombstone may propagate deletion.
    if base.is_some() && local.is_none() {
        // Purging a profile after its deletion was confirmed is already settled;
        // it must neither generate a new deletion nor block pending session cleanup.
        if base == remote && base.is_some_and(|b| b["deleted"] == true) {
            return Decision::Unchanged;
        }
        return Decision::Conflict(vec!["local_config_missing".into()]);
    }
    if local == remote {
        return Decision::Unchanged;
    }
    if base.is_some() && remote.is_none() {
        return Decision::Conflict(vec!["cloud_object_missing".into()]);
    }
    match (base, local, remote) {
        (None, Some(l), None) => Decision::Upload(l.clone()),
        (None, None, Some(r)) => Decision::Apply(r.clone()),
        (None, Some(_), Some(_)) => Decision::Conflict(vec!["first_association".into()]),
        (Some(b), Some(l), Some(r)) => {
            if (r["deleted"] == true && l["deleted"] != true)
                || (l["deleted"] == true && l != b && r != b && r["deleted"] != true)
            {
                return Decision::Conflict(vec!["deletion_vs_edit".into()]);
            }
            if l == b {
                return Decision::Apply(r.clone());
            };
            if r == b {
                return Decision::Upload(l.clone());
            };
            let result = merge::merge(b, l, r);
            if result.conflicts.is_empty() {
                Decision::Upload(result.value)
            } else {
                Decision::Conflict(result.conflicts)
            }
        }
        _ => Decision::Unchanged,
    }
}
fn value(doc: &Document) -> Result<Value, String> {
    serde_json::to_value(doc).map_err(|_| "无法读取副本配置".into())
}
fn validate_object(o: &Object) -> Result<(), String> {
    if uuid::Uuid::parse_str(&o.id).is_err()
        || uuid::Uuid::parse_str(&o.writer).is_err()
        || !matches!(o.kind.as_str(), "config" | "session")
        || o.revision == 0
        || o.epoch == 0
    {
        return Err("云端记录标识无效".into());
    };
    Ok(())
}
// Compare authenticated identity/version fields; server timestamps are not part of AAD.
pub(crate) fn check_successor(base: &Object, remote: &Object) -> Result<(), String> {
    if remote.id != base.id
        || remote.kind != base.kind
        || remote.revision < base.revision
        || remote.epoch < base.epoch
    {
        return Err("云端记录版本倒退，已保留本地数据；请检查云端备份或服务完整性".into());
    }
    if remote.revision == base.revision
        && (remote.epoch != base.epoch
            || remote.deleted != base.deleted
            || remote.writer != base.writer
            || remote.generation != base.generation
            || (!remote.payload.is_empty()
                && !base.payload.is_empty()
                && remote.payload != base.payload))
    {
        return Err("同一云端版本的内容发生变化，已停止同步并保留本地数据".into());
    }
    Ok(())
}
fn check_confirmed(c: &Connection, remote: &Object) -> Result<(), String> {
    let account = c.account_id()?;
    for binding in c.disk.accounts[&account]
        .bindings
        .values()
        .filter(|b| b.cloud_id == remote.id)
    {
        let bases: Vec<&Object> = if remote.kind == "config" {
            binding.baseline.iter().collect()
        } else {
            binding
                .session_baseline
                .iter()
                .chain(binding.session_checkpoint.iter())
                .collect()
        };
        for base in bases {
            check_successor(base, remote)?;
            if remote.kind == "session"
                && !remote.payload.is_empty()
                && remote.revision == base.revision
            {
                check_successor(&super::session_sync::cached(c, base)?, remote)?;
            }
        }
    }
    Ok(())
}
pub fn manifest(c: &mut Connection) -> Result<Vec<Object>, String> {
    let v = c.request(Method::GET, "/v1/objects", None)?;
    let list: Vec<Object> =
        serde_json::from_value(v["objects"].clone()).map_err(|_| "云端清单格式无效")?;
    if list.len() > 1000 {
        return Err("云端副本数量超过限制".into());
    };
    for o in &list {
        validate_object(o)?;
        check_confirmed(c, o)?;
    }
    if let Some(guard) = &c.mcp_versions {
        guard.manifest(&list)?;
    }
    Ok(list)
}
pub fn fetch(c: &mut Connection, id: &str, kind: &str) -> Result<Object, String> {
    if uuid::Uuid::parse_str(id).is_err() || !matches!(kind, "config" | "session") {
        return Err("云端标识无效".into());
    };
    let v = c.request(Method::GET, &format!("/v1/objects/{id}?kind={kind}"), None)?;
    let o: Object = serde_json::from_value(v).map_err(|_| "云端记录格式无效")?;
    validate_object(&o)?;
    if o.id != id || o.kind != kind {
        return Err("云端记录身份不匹配".into());
    };
    check_confirmed(c, &o)?;
    if let Some(guard) = &c.mcp_versions {
        guard.check(id, kind, Some(&o))?;
    }
    Ok(o)
}
pub fn decrypt(c: &Connection, o: &Object) -> Result<Zeroizing<Vec<u8>>, String> {
    c.unlocked_root()?;
    validate_object(o)?;
    let id = c.account_id()?;
    let root = super::vault::root(&c.ns, &id, o.epoch)?.ok_or("缺少此版本的解密密钥")?;
    let envelope = serde_json::from_str(&o.payload).map_err(|_| "云端密文格式无效")?;
    crypto::open(&root, &o.context(&id), &envelope)
}
pub fn document(c: &Connection, o: &Object) -> Result<Document, String> {
    let plain = decrypt(c, o)?;
    let doc = Document::parse(serde_json::from_slice(&plain).map_err(|_| "云端副本配置无效")?)?;
    if doc.deleted != o.deleted {
        return Err("云端删除状态校验失败".into());
    };
    Ok(doc)
}
pub fn upload(
    c: &mut Connection,
    id: &str,
    kind: &str,
    base: u64,
    deleted: bool,
    generation: u64,
    restore: bool,
    plain: &[u8],
) -> Result<Object, String> {
    let account = c.account_id()?;
    let epoch = c.disk.accounts[&account].epoch;
    let root = c.unlocked_root()?;
    let mut object = Object {
        id: id.into(),
        kind: kind.into(),
        revision: base + 1,
        epoch,
        deleted,
        payload: String::new(),
        writer: c.device.id.clone(),
        generation,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    object.payload = serde_json::to_string(&crypto::seal(&root, &object.context(&account), plain)?)
        .map_err(|_| "无法生成同步密文")?;
    let mut body = json!({"kind":kind,"baseRevision":base,"epoch":epoch,"deleted":deleted,"restore":restore,"generation":generation,"payload":object.payload});
    if kind == "session" {
        let config = c.disk.accounts[&account]
            .bindings
            .values()
            .find(|b| b.cloud_id == id)
            .and_then(|b| b.baseline.as_ref())
            .ok_or("缺少已同步的副本配置，请先同步配置")?;
        body["configRevision"] = json!(config.revision);
    }
    let response = c.request(Method::PUT, &format!("/v1/objects/{id}"), Some(&body))?;
    if response["revision"] != object.revision {
        return Err("云端版本校验失败".into());
    };
    if let Some(guard) = &mut c.mcp_versions {
        guard.record(&object);
    }
    Ok(object)
}
fn put_document(
    c: &mut Connection,
    b: &Binding,
    doc: &Document,
    base: Option<&Object>,
    restore: bool,
) -> Result<Object, String> {
    let bytes = Zeroizing::new(serde_json::to_vec(doc).map_err(|_| "无法读取副本配置")?);
    upload(
        c,
        &b.cloud_id,
        "config",
        base.map(|o| o.revision).unwrap_or(0),
        doc.deleted,
        0,
        restore,
        &bytes,
    )
}
fn apply(
    c: &Connection,
    local_id: &str,
    doc: &Document,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
    let existing = profiles::load(&c.root)?
        .into_iter()
        .find(|p| p.id == local_id);
    if doc.deleted {
        if let Some(p) = existing {
            if p.deleted_at.is_none() {
                profiles::remove(&c.root, local_id, false, runtime)?
            }
        };
        return Ok(());
    }
    if let Some(p) = existing {
        if p.deleted_at.is_some() {
            profiles::restore(&c.root, local_id)?
        };
        profiles::update(&c.root, local_id, doc.draft())?;
    } else {
        profiles::create_synced(&c.root, doc.draft(), local_id)?;
    };
    Ok(())
}
fn bind_cloud(c: &mut Connection, o: &Object, runtime: &profiles::Runtime) -> Result<(), String> {
    let account = c.account_id()?;
    let local = o.id.replace('-', "");
    c.disk.can_attach(&account, &local)?;
    let doc = document(c, o)?;
    if profiles::load(&c.root)?.iter().any(|p| p.id == local)
        && !c.disk.accounts[&account].bindings.contains_key(&local)
    {
        return Err("云端与本地副本标识冲突，请保留两份并重新关联".into());
    }
    let state = c.disk.accounts.get_mut(&account).unwrap();
    state
        .bindings
        .entry(local.clone())
        .or_insert_with(|| Binding::new(&o.id));
    state.known_local.insert(local.clone());
    c.disk.save(&c.root)?;
    apply(c, &local, &doc, runtime)?;
    c.disk
        .accounts
        .get_mut(&account)
        .unwrap()
        .bindings
        .get_mut(&local)
        .unwrap()
        .baseline = Some(o.clone());
    c.disk.save(&c.root)
}
pub fn preview(c: &mut Connection) -> Result<Value, String> {
    c.unlocked_root()?;
    let account = c.account_id()?;
    let list = manifest(c)?;
    let mut out = Vec::new();
    for o in list.into_iter().filter(|o| o.kind == "config") {
        let object = fetch(c, &o.id, "config")?;
        let doc = document(c, &object)?;
        out.push(json!({"id":o.id,"name":doc.name,"browser":doc.browser,"deleted":doc.deleted,"revision":object.revision}))
    }
    let local=profiles::load(&c.root)?.into_iter().filter(|p|p.deleted_at.is_none()).map(|p|json!({"id":p.id,"name":p.name,"browser":p.browser,"eligible":p.received_share.is_none()&&c.disk.can_attach(&account,&p.id).is_ok(),"selected":c.disk.accounts[&account].bindings.contains_key(&p.id)&&!c.disk.accounts[&account].detached.contains(&p.id)})).collect::<Vec<_>>();
    Ok(json!({"profiles":out,"localProfiles":local}))
}
pub fn setup(
    c: &mut Connection,
    mode: &str,
    local: &[String],
    cloud: &[String],
    sessions: bool,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
    if !matches!(mode, "merge" | "upload" | "none") {
        return Err("同步方式无效".into());
    };
    let account = c.account_id()?;
    if mode != "none" {
        c.unlocked_root()?;
        if !c.disk.accounts[&account].recovery_saved {
            return Err("请先确认保存恢复密钥".into());
        }
    }
    let all = profiles::load(&c.root)?;
    for id in local {
        super::sharing::editable(&c.root, id)?;
        c.disk.can_attach(&account, id)?;
        if !all.iter().any(|p| &p.id == id && p.deleted_at.is_none()) {
            return Err("所选副本不存在".into());
        }
    }
    let initial_cloud = if mode == "merge" {
        manifest(c)?
    } else {
        Vec::new()
    };
    {
        let state = c.disk.accounts.get_mut(&account).unwrap();
        state.auto_sync = false;
        state.known_local.extend(all.iter().map(|p| p.id.clone()));
        if mode != "none" {
            for (id, b) in &state.bindings {
                if !local.contains(id) {
                    state.detached.insert(id.clone());
                    state.excluded.insert(b.cloud_id.clone());
                }
            }
            for id in local {
                let b = state
                    .bindings
                    .entry(id.clone())
                    .or_insert_with(|| Binding::new(&uuid::Uuid::new_v4().to_string()));
                state.excluded.remove(&b.cloud_id);
                state.detached.remove(id);
            }
        }
        if mode == "merge" {
            for o in initial_cloud.iter().filter(|o| o.kind == "config") {
                if cloud.contains(&o.id) {
                    state.excluded.remove(&o.id);
                } else if !state.bindings.values().any(|b| b.cloud_id == o.id) {
                    state.excluded.insert(o.id.clone());
                }
            }
        }
    }
    c.disk.save(&c.root)?;
    if mode == "merge" {
        for id in cloud {
            let o = fetch(c, id, "config")?;
            if o.deleted {
                continue;
            };
            let existing = c.disk.accounts[&account]
                .bindings
                .iter()
                .find(|(_, b)| b.cloud_id == *id)
                .map(|(local, _)| local.clone());
            if let Some(local) = existing {
                let state = c.disk.accounts.get_mut(&account).unwrap();
                state.detached.remove(&local);
                state.excluded.remove(id);
                if let Some(p) = all.iter().find(|p| p.id == local) {
                    if p.deleted_at.is_some() {
                        profiles::restore(&c.root, &local)?
                    }
                } else {
                    let doc = document(c, &o)?;
                    let mut b = c.disk.accounts[&account].bindings[&local].clone();
                    stage_config_restore(c, &account, &local, &mut b)?;
                    apply(c, &local, &doc, runtime)?;
                    b.baseline = Some(o);
                    c.disk.accounts.get_mut(&account).unwrap().bindings.insert(local.clone(), b);
                }
                c.disk.save(&c.root)?;
            } else {
                bind_cloud(c, &o, runtime)?
            }
        }
    }
    {
        let s = c.disk.accounts.get_mut(&account).unwrap();
        s.setup_complete = true;
        s.auto_sync = mode != "none";
        s.pull_cloud = mode == "merge";
        s.session_sync = sessions && mode != "none";
    }
    c.disk.save(&c.root)?;
    if mode != "none" {
        run(c, runtime)?
    };
    Ok(())
}
fn conflict(
    account: &str,
    c: &mut Connection,
    local: &str,
    b: &Binding,
    name: &str,
    kind: &str,
    fields: Vec<String>,
    rev: u64,
) {
    c.disk
        .accounts
        .get_mut(account)
        .unwrap()
        .conflicts
        .push(Conflict {
            local_id: local.into(),
            cloud_id: b.cloud_id.clone(),
            name: name.into(),
            kind: kind.into(),
            fields,
            remote_revision: rev,
        })
}
/// Reconcile one configuration; an optimistic CAS race is retried using fresh
/// cloud data, without applying an uncommitted merge to the local profile.
pub(super) fn config_profile(
    c: &mut Connection,
    id: &str,
    remote: Option<&Object>,
    runtime: &profiles::Runtime,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let account = c.account_id()?;
    let mut b = c.disk.accounts[&account].bindings[id].clone();
    let p = profiles::load(&c.root)?.into_iter().find(|p| p.id == id);
    let lv = p.as_ref().map(Document::export).transpose()?.as_ref().map(value).transpose()?;
    let bv = b.baseline.as_ref().map(|o| document(c, o)).transpose()?.as_ref().map(value).transpose()?;
    let mut ro = remote.map(|o| fetch(c, &o.id, "config")).transpose()?;
    c.disk.accounts.get_mut(&account).unwrap().conflicts.retain(|x| x.local_id != id || x.kind == "session");
    for attempt in 0..3 {
        c.check_mcp_deadline()?;
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("CANCELLED".into());
        }
        let rv = ro.as_ref().map(|o| document(c, o)).transpose()?.as_ref().map(value).transpose()?;
        let name = lv.as_ref().or(rv.as_ref()).or(bv.as_ref())
            .and_then(|v| v["name"].as_str()).unwrap_or("待恢复副本");
        match decide(bv.as_ref(), lv.as_ref(), rv.as_ref()) {
            Decision::Conflict(fields) => {
                conflict(&account, c, id, &b, name, "config", fields, ro.as_ref().map(|o|o.revision).unwrap_or(0));
            }
            Decision::Unchanged => {
                if ro.is_some() { b.baseline = ro; }
                b.pending_config = None;
            }
            Decision::Apply(v) => {
                let doc = Document::parse(v)?;
                if doc.deleted && p.as_ref().is_some_and(|p| profiles::running(&c.root,p).unwrap_or(true)) {
                    b.pending_config = ro.clone();
                    conflict(&account,c,id,&b,name,"running",vec!["close_browser".into()],ro.as_ref().map(|o|o.revision).unwrap_or(0));
                } else {
                    apply(c,id,&doc,runtime)?;
                    b.baseline = ro;
                    b.pending_config = None;
                }
            }
            Decision::Upload(v) => {
                let doc = Document::parse(v.clone())?;
                let restore = b.baseline.as_ref().is_some_and(|o|o.deleted) && !doc.deleted;
                match put_document(c,&b,&doc,ro.as_ref(),restore) {
                    Ok(o) => {
                        if lv.as_ref() != Some(&v) { apply(c,id,&doc,runtime)?; }
                        b.baseline = Some(o);
                        b.pending_config = None;
                    }
                    Err(e) if e.contains("revision_conflict") => {
                        if attempt < 2 {
                            ro = Some(fetch(c,&b.cloud_id,"config")?);
                            continue;
                        }
                        conflict(&account,c,id,&b,name,"config",vec!["concurrent_write".into()],ro.as_ref().map(|o|o.revision).unwrap_or(0));
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        break;
    }
    c.disk.accounts.get_mut(&account).unwrap().bindings.insert(id.into(),b);
    c.disk.save(&c.root)
}
pub fn run(c: &mut Connection, runtime: &profiles::Runtime) -> Result<(), String> {
    run_selected(c, runtime, None, &std::sync::atomic::AtomicBool::new(false))
}
pub(crate) fn run_selected(
    c: &mut Connection,
    runtime: &profiles::Runtime,
    selected: Option<&[String]>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    let account = c.account_id()?;
    if !c.disk.accounts[&account].setup_complete {
        return Err("请先选择同步方式和副本".into());
    };
    c.refresh_vault()?;
    c.unlocked_root()?;
    if !c.disk.accounts[&account].recovery_saved {
        return Err("请先保存当前恢复密钥".into());
    }
    let remote: BTreeMap<String, Object> = manifest(c)?
        .into_iter()
        .filter(|o| o.kind == "config")
        .map(|o| (o.id.clone(), o))
        .collect();
    let local = profiles::load(&c.root)?;
    for p in &local {
        if selected.is_some() {
            continue;
        }
        if super::sharing::restricted(&c.root, &p.id)? {
            continue;
        }
        if !c.disk.accounts[&account].known_local.contains(&p.id)
            && !c.disk.accounts[&account].bindings.contains_key(&p.id)
            && p.deleted_at.is_none()
            && c.disk.can_attach(&account, &p.id).is_ok()
        {
            let s = c.disk.accounts.get_mut(&account).unwrap();
            s.bindings.insert(
                p.id.clone(),
                Binding::new(&uuid::Uuid::new_v4().to_string()),
            );
            s.known_local.insert(p.id.clone());
        }
    }
    c.disk
        .accounts
        .get_mut(&account)
        .unwrap()
        .conflicts
        .retain(|x| x.kind == "session" || selected.is_some_and(|ids| !ids.contains(&x.local_id)));
    c.disk.save(&c.root)?;
    let bindings = c.disk.accounts[&account].bindings.clone();
    for (id, b) in bindings {
        if selected.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        c.check_mcp_deadline()?;
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("CANCELLED".into());
        }
        if super::sharing::restricted(&c.root, &id)? {
            continue;
        }
        if c.disk.accounts[&account].detached.contains(&id) {
            continue;
        }
        config_profile(c, &id, remote.get(&b.cloud_id), runtime, cancel)?;
    }
    if selected.is_none() && c.disk.accounts[&account].pull_cloud {
        for o in remote.values() {
            let s = &c.disk.accounts[&account];
            if !o.deleted
                && !s.excluded.contains(&o.id)
                && !s.bindings.values().any(|b| b.cloud_id == o.id)
            {
                let object = fetch(c, &o.id, "config")?;
                bind_cloud(c, &object, runtime)?
            }
        }
    }
    if selected.is_none() {
        super::session_sync::run(c)?;
    } else {
        super::session_sync::run_selected(c, selected, cancel)?;
    }
    let s = c.disk.accounts.get_mut(&account).unwrap();
    s.last_sync = Some(chrono::Utc::now().to_rfc3339());
    s.last_error = None;
    c.disk.save(&c.root)?;
    super::session_sync::prune_cache(c)
}
fn stage_config_restore(c: &mut Connection, account: &str, local: &str, b: &mut Binding) -> Result<(), String> {
    // Keep the authenticated high-water mark, but no longer assume the recovered
    // directory is the old local session. Empty local data can pull the cloud
    // session; differing nonempty data requires a separate explicit choice.
    b.session_checkpoint = b.session_baseline.take().or(b.session_checkpoint.take());
    b.session_hash = None;
    b.file_stamp = None;
    b.session_dirty = false;
    b.pending_session = None;
    b.generation = 0;
    // This must be durable before apply() can recreate profiles.json. If apply
    // or the final baseline save fails, a restart still cannot publish an empty
    // recreated directory as an edit against the previous session baseline.
    c.disk.accounts.get_mut(account).unwrap().bindings.insert(local.into(), b.clone());
    c.disk.save(&c.root)
}
pub fn resolve(
    c: &mut Connection,
    id: &str,
    choice: &str,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
    super::sharing::editable(&c.root, id)?;
    let account = c.account_id()?;
    let item = c.disk.accounts[&account]
        .conflicts
        .iter()
        .find(|x| x.local_id == id && x.kind == "config")
        .cloned()
        .ok_or("冲突已变化，请重新同步")?;
    if choice == "skip" {
        return Ok(());
    };
    let mut binding = c.disk.accounts[&account].bindings[id].clone();
    let remote = fetch(c, &binding.cloud_id, "config")?;
    if remote.revision != item.remote_revision {
        return Err("云端版本再次变化，请重新同步后处理冲突".into());
    }
    let remote_doc = document(c, &remote)?;
    let local = profiles::load(&c.root)?.into_iter().find(|p| p.id == id);
    let local_doc = local
        .as_ref()
        .map(Document::export)
        .transpose()?;
    match choice {
        "local" => {
            let local_doc = local_doc.as_ref().ok_or("本地配置记录缺失，不能以删除记录代替本地配置。请检查数据目录或从备份恢复后重试。")?;
            binding.baseline = Some(put_document(
                c,
                &binding,
                local_doc,
                Some(&remote),
                remote.deleted && !local_doc.deleted,
            )?);
        }
        "cloud" => {
            if local.is_none() && !remote_doc.deleted {
                stage_config_restore(c, &account, id, &mut binding)?;
            }
            apply(c, id, &remote_doc, runtime)?;
            binding.baseline = Some(remote);
        }
        "both" => {
            let p = local.as_ref().ok_or("本地副本不存在，无法保留两份")?;
            platform::ensure_profile_closed(&profiles::path(&c.root, p)?)?;
            let mut d = local_doc.as_ref().ok_or("本地配置不存在")?.draft();
            d.name = format!("{} (copy)", d.name.chars().take(50).collect::<String>());
            let copied = if c.disk.accounts[&account].session_sync {
                super::session_sync::copy_with_session(c, p, d, runtime)?
            } else {
                profiles::create(&c.root, d, Some(id))?
            };
            let s = c.disk.accounts.get_mut(&account).unwrap();
            s.bindings.insert(
                copied.id.clone(),
                Binding::new(&uuid::Uuid::new_v4().to_string()),
            );
            s.known_local.insert(copied.id);
            c.disk.save(&c.root)?;
            apply(c, id, &remote_doc, runtime)?;
            binding.baseline = Some(remote);
        }
        _ => return Err("冲突处理方式无效".into()),
    };
    let s = c.disk.accounts.get_mut(&account).unwrap();
    s.bindings.insert(id.into(), binding);
    s.conflicts
        .retain(|x| x.local_id != id || x.kind != "config");
    c.disk.save(&c.root)
}
pub fn detach(
    c: &mut Connection,
    id: &str,
    remove_local: bool,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
    let account = c.account_id()?;
    let cloud = c.disk.accounts[&account]
        .bindings
        .get(id)
        .ok_or("副本未关联云端")?
        .cloud_id
        .clone();
    if remove_local {
        profiles::remove(&c.root, id, false, runtime)?
    };
    let state = c.disk.accounts.get_mut(&account).unwrap();
    state.excluded.insert(cloud);
    state.detached.insert(id.into());
    state.conflicts.retain(|x| x.local_id != id);
    c.disk.save(&c.root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_versions_reject_changes_between_preview_and_execution() {
        let original = Object {
            id: "one".into(),
            kind: "config".into(),
            revision: 3,
            epoch: 1,
            deleted: false,
            payload: String::new(),
            writer: "writer".into(),
            generation: 0,
            updated_at: String::new(),
        };
        let mut guard = VersionGuard {
            ids: vec!["one".into()],
            objects: vec![original.clone()],
        };
        assert!(guard.check("one", "config", Some(&original)).is_ok());
        let mut newer = original.clone();
        newer.revision += 1;
        assert!(guard.check("one", "config", Some(&newer)).is_err());
        assert!(guard.check("one", "config", None).is_err());
        let mut session = original.clone();
        session.kind = "session".into();
        assert!(guard.manifest(&[original.clone(), session]).is_err());
        guard.record(&newer);
        assert!(guard.check("one", "config", Some(&newer)).is_ok());
        assert!(guard.check("unselected", "config", Some(&original)).is_ok());
    }
    use serde_json::json;
    #[test]
    fn authentic_old_ciphertext_cannot_regress_a_confirmed_object() {
        let key = crypto::random_key().unwrap();
        let account = uuid::Uuid::new_v4().to_string();
        for kind in ["config", "session"] {
            let mut old = Object {
                id: uuid::Uuid::new_v4().to_string(),
                kind: kind.into(),
                revision: 1,
                epoch: 1,
                deleted: false,
                payload: String::new(),
                writer: uuid::Uuid::new_v4().to_string(),
                generation: 0,
                updated_at: String::new(),
            };
            old.payload = serde_json::to_string(
                &crypto::seal(&key, &old.context(&account), b"old state").unwrap(),
            )
            .unwrap();
            let envelope = serde_json::from_str(&old.payload).unwrap();
            assert_eq!(
                crypto::open(&key, &old.context(&account), &envelope)
                    .unwrap()
                    .as_slice(),
                b"old state"
            );
            let mut confirmed = old.clone();
            confirmed.revision = 2;
            assert!(check_successor(&confirmed, &old).is_err());
            assert!(check_successor(&old, &confirmed).is_ok());
            let mut changed = old.clone();
            changed.payload.push(' ');
            assert!(check_successor(&old, &changed).is_err());
            let mut timestamp = old.clone();
            timestamp.updated_at = "server timestamp".into();
            assert!(check_successor(&old, &timestamp).is_ok());
        }
    }
    #[test]
    fn deleted_remote_cannot_silently_delete_edited_local() {
        let base = json!({"name":"old","deleted":false});
        let local = json!({"name":"new","deleted":false});
        let remote = json!({"name":"old","deleted":true});
        assert!(matches!(
            decide(Some(&base), Some(&local), Some(&remote)),
            Decision::Conflict(_)
        ));
    }
    #[test]
    fn cloud_absence_is_not_deletion_on_first_sync() {
        let local = json!({"name":"a","deleted":false});
        assert!(matches!(
            decide(None, Some(&local), None),
            Decision::Upload(_)
        ));
    }
    #[test]
    fn missing_previously_synced_object_requires_attention() {
        let base = json!({"name":"a","deleted":false});
        assert!(matches!(
            decide(Some(&base), Some(&base), None),
            Decision::Conflict(_)
        ));
    }
    #[test]
    fn live_local_requires_confirmation_before_accepting_remote_deletion() {
        let base = json!({"name":"a","deleted":false});
        let remote = json!({"name":"a","deleted":true});
        assert!(matches!(
            decide(Some(&base), Some(&base), Some(&remote)),
            Decision::Conflict(_)
        ));
        let edited = json!({"name":"b","deleted":false});
        assert!(matches!(
            decide(Some(&base), None, Some(&edited)),
            Decision::Conflict(_)
        ));
    }
    #[test]
    fn absent_local_registry_entry_never_generates_cloud_deletion() {
        let base = json!({"name":"a","deleted":false});
        let deleted = json!({"name":"a","deleted":true});
        for remote in [&base, &deleted] {
            assert!(matches!(
                decide(Some(&base), None, Some(remote)),
                Decision::Conflict(fields) if fields == ["local_config_missing"]
            ));
        }
        let restored = json!({"name":"restored","deleted":false});
        assert!(matches!(decide(Some(&deleted), None, Some(&restored)),
            Decision::Conflict(fields) if fields == ["local_config_missing"]));
    }
    #[test]
    fn purging_after_confirmed_cloud_deletion_does_not_recreate_a_conflict() {
        let deleted = json!({"name":"a","deleted":true});
        assert!(matches!(decide(Some(&deleted), None, Some(&deleted)), Decision::Unchanged));
    }
    #[test]
    fn explicit_local_tombstone_still_synchronizes_deletion() {
        let base = json!({"name":"a","deleted":false});
        let deleted = json!({"name":"a","deleted":true});
        assert!(matches!(
            decide(Some(&base), Some(&deleted), Some(&base)),
            Decision::Upload(value) if value == deleted
        ));
        assert!(matches!(
            decide(Some(&deleted), Some(&deleted), Some(&deleted)),
            Decision::Unchanged
        ));
    }
    #[test]
    fn explicit_cloud_restore_reaches_device_that_already_confirmed_deletion() {
        let deleted=json!({"name":"a","deleted":true});
        let restored=json!({"name":"a","deleted":false});
        assert!(matches!(decide(Some(&deleted),Some(&deleted),Some(&restored)),
            Decision::Apply(value) if value==restored));
        let live=json!({"name":"a","deleted":false});
        let edited=json!({"name":"remote edit","deleted":false});
        assert!(matches!(decide(Some(&live),Some(&deleted),Some(&edited)),Decision::Conflict(_)));
    }
    #[test]
    fn purge_keeps_explicit_deletion_intent_until_cloud_acknowledges_it() {
        let temp = tempfile::tempdir().unwrap();
        // macOS temporary directories may be reached through /var -> /private/var.
        // Exercise the profile guard against the real root, as the app does.
        let root = temp.path().canonicalize().unwrap();
        let draft = profiles::Draft { name:"Pending deletion".into(), browser:"firefox".into(),
            notes:String::new(), tags:vec![], proxy:crate::proxy::ProxyConfig::system(),
            password:None, clear_password:false, preferences:profiles::Preferences::default() };
        let id = uuid::Uuid::new_v4().simple().to_string();
        let p = profiles::create_synced(&root, draft, &id).unwrap();
        let mut disk = super::super::model::Disk::default();
        let account = uuid::Uuid::new_v4().to_string();
        let cloud_id = uuid::Uuid::new_v4().to_string();
        let mut binding = Binding::new(&cloud_id);
        binding.baseline = Some(Object { id:cloud_id, kind:"config".into(), revision:1,
            epoch:1, deleted:false, payload:String::new(), writer:uuid::Uuid::new_v4().to_string(),
            generation:0, updated_at:String::new() });
        disk.accounts.entry(account.clone()).or_default().bindings.insert(id.clone(), binding);
        disk.save(&root).unwrap();
        let runtime = profiles::Runtime::default();
        profiles::remove(&root, &id, false, &runtime).unwrap();
        assert!(profiles::remove(&root, &id, true, &runtime).is_err());
        assert!(profiles::get(&root, &id, true).unwrap().deleted_at.is_some());
        assert!(profiles::path(&root, &p).unwrap().exists());
        disk.accounts.get_mut(&account).unwrap().bindings.get_mut(&id).unwrap().baseline.as_mut().unwrap().deleted = true;
        disk.save(&root).unwrap();
        profiles::remove(&root, &id, true, &runtime).unwrap();
        assert!(profiles::get(&root, &id, true).is_err());
    }
}
