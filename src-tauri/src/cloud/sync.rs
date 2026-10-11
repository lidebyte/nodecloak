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
pub fn decide(base: Option<&Value>, local: Option<&Value>, remote: Option<&Value>) -> Decision {
    let deleted_local = base.map(|b| {
        let mut v = b.clone();
        v["deleted"] = json!(true);
        v
    });
    let local = local.or(deleted_local.as_ref());
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
            if (r["deleted"] == true && l != b && l["deleted"] != true)
                || (l["deleted"] == true && r != b && r["deleted"] != true)
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
    let local=profiles::load(&c.root)?.into_iter().filter(|p|p.deleted_at.is_none()).map(|p|json!({"id":p.id,"name":p.name,"browser":p.browser,"eligible":c.disk.can_attach(&account,&p.id).is_ok(),"selected":c.disk.accounts[&account].bindings.contains_key(&p.id)&&!c.disk.accounts[&account].detached.contains(&p.id)})).collect::<Vec<_>>();
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
                    apply(c, &local, &doc, runtime)?;
                    let b = c
                        .disk
                        .accounts
                        .get_mut(&account)
                        .unwrap()
                        .bindings
                        .get_mut(&local)
                        .unwrap();
                    b.baseline = Some(o);
                    b.session_checkpoint =
                        b.session_baseline.take().or(b.session_checkpoint.take());
                    b.session_hash = None;
                    b.file_stamp = None;
                    b.session_dirty = false;
                    b.generation = 0;
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
pub fn run(c: &mut Connection, runtime: &profiles::Runtime) -> Result<(), String> {
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
        .retain(|x| x.kind == "session");
    c.disk.save(&c.root)?;
    let bindings = c.disk.accounts[&account].bindings.clone();
    for (id, mut b) in bindings {
        if c.disk.accounts[&account].detached.contains(&id) {
            continue;
        }
        let p = local.iter().find(|p| p.id == id);
        let lv = p
            .map(Document::export)
            .transpose()?
            .as_ref()
            .map(value)
            .transpose()?;
        let bv = b
            .baseline
            .as_ref()
            .map(|o| document(c, o))
            .transpose()?
            .as_ref()
            .map(value)
            .transpose()?;
        let ro = remote
            .get(&b.cloud_id)
            .map(|o| fetch(c, &o.id, "config"))
            .transpose()?;
        let rv = ro
            .as_ref()
            .map(|o| document(c, o))
            .transpose()?
            .as_ref()
            .map(value)
            .transpose()?;
        let name = p.map(|p| p.name.as_str()).unwrap_or("已删除副本");
        match decide(bv.as_ref(), lv.as_ref(), rv.as_ref()) {
            Decision::Conflict(fields) => conflict(
                &account,
                c,
                &id,
                &b,
                name,
                "config",
                fields,
                ro.as_ref().map(|o| o.revision).unwrap_or(0),
            ),
            Decision::Unchanged => {
                if ro.is_some() {
                    b.baseline = ro;
                }
            }
            Decision::Apply(v) => {
                let doc = Document::parse(v)?;
                if doc.deleted && p.is_some_and(|p| profiles::running(&c.root, p).unwrap_or(true)) {
                    b.pending_config = ro.clone();
                    conflict(
                        &account,
                        c,
                        &id,
                        &b,
                        name,
                        "running",
                        vec!["close_browser".into()],
                        ro.as_ref().map(|o| o.revision).unwrap_or(0),
                    );
                } else {
                    apply(c, &id, &doc, runtime)?;
                    b.baseline = ro;
                    b.pending_config = None;
                }
            }
            Decision::Upload(v) => {
                let doc = Document::parse(v.clone())?;
                if lv.as_ref() != Some(&v) {
                    apply(c, &id, &doc, runtime)?;
                }
                let restore = b.baseline.as_ref().is_some_and(|o| o.deleted) && !doc.deleted;
                match put_document(c, &b, &doc, ro.as_ref(), restore) {
                    Ok(o) => {
                        b.baseline = Some(o);
                        b.pending_config = None
                    }
                    Err(e) if e.contains("revision_conflict") => conflict(
                        &account,
                        c,
                        &id,
                        &b,
                        name,
                        "config",
                        vec!["concurrent_write".into()],
                        ro.as_ref().map(|o| o.revision).unwrap_or(0),
                    ),
                    Err(e) => return Err(e),
                }
            }
        };
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(id, b);
        c.disk.save(&c.root)?;
    }
    if c.disk.accounts[&account].pull_cloud {
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
    super::session_sync::run(c)?;
    let s = c.disk.accounts.get_mut(&account).unwrap();
    s.last_sync = Some(chrono::Utc::now().to_rfc3339());
    s.last_error = None;
    c.disk.save(&c.root)?;
    super::session_sync::prune_cache(c)
}
pub fn resolve(
    c: &mut Connection,
    id: &str,
    choice: &str,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
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
        .transpose()?
        .or_else(|| {
            binding
                .baseline
                .as_ref()
                .and_then(|o| document(c, o).ok())
                .map(|mut d| {
                    d.deleted = true;
                    d
                })
        })
        .ok_or("本地配置不存在")?;
    match choice {
        "local" => {
            binding.baseline = Some(put_document(
                c,
                &binding,
                &local_doc,
                Some(&remote),
                remote.deleted && !local_doc.deleted,
            )?);
        }
        "cloud" => {
            apply(c, id, &remote_doc, runtime)?;
            binding.baseline = Some(remote);
        }
        "both" => {
            let p = local.as_ref().ok_or("本地副本不存在，无法保留两份")?;
            platform::ensure_profile_closed(&profiles::path(&c.root, p)?)?;
            let mut d = local_doc.draft();
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
    fn unmodified_local_receives_tombstone_and_edit_vs_delete_conflicts() {
        let base = json!({"name":"a","deleted":false});
        let remote = json!({"name":"a","deleted":true});
        assert!(matches!(
            decide(Some(&base), Some(&base), Some(&remote)),
            Decision::Apply(_)
        ));
        let edited = json!({"name":"b","deleted":false});
        assert!(matches!(
            decide(Some(&base), None, Some(&edited)),
            Decision::Conflict(_)
        ));
    }
}
