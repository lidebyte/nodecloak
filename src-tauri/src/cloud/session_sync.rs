use super::{
    account::Connection,
    browser_session::{self, Snapshot},
    model::{Binding, Conflict, Object},
    sync,
};
use crate::{engine, platform, profiles};
use reqwest::Method;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn cache_path(c: &Connection, o: &Object) -> Result<PathBuf, String> {
    let account = c.account_id()?;
    if uuid::Uuid::parse_str(&o.id).is_err() || o.kind != "session" {
        return Err("登录记录标识无效".into());
    };
    Ok(c.root
        .join("cloud")
        .join(account)
        .join(&o.id)
        .join(format!("session-{}.json", o.revision)))
}
pub(crate) fn cache(c: &Connection, o: &Object) -> Result<Object, String> {
    let file = cache_path(c, o)?;
    profiles::guard_tree(&file)?;
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|_| "无法保存加密登录记录")?;
    engine::atomic_write(&file, &serde_json::to_vec(o).map_err(|_| "登录记录无效")?)?;
    let mut metadata = o.clone();
    metadata.payload.clear();
    Ok(metadata)
}
pub(crate) fn cached(c: &Connection, o: &Object) -> Result<Object, String> {
    let file = cache_path(c, o)?;
    profiles::guard_tree(&file)?;
    let b = std::fs::read(file).map_err(|_| "本机加密基线缺失，请保留本地数据并重新同步")?;
    if b.len() > 12 << 20 {
        return Err("本机登录记录过大".into());
    };
    let full: Object = serde_json::from_slice(&b).map_err(|_| "本机登录记录损坏")?;
    if full.id != o.id
        || full.revision != o.revision
        || full.epoch != o.epoch
        || full.kind != o.kind
    {
        return Err("本机登录记录身份不匹配".into());
    };
    Ok(full)
}
// Prune only superseded encrypted revisions after the committed baseline is durable.
pub(crate) fn prune_cache(c: &Connection) -> Result<(), String> {
    let account = c.account_id()?;
    for b in c.disk.accounts[&account].bindings.values() {
        let directory = c.root.join("cloud").join(&account).join(&b.cloud_id);
        profiles::guard_tree(&directory)?;
        if !directory.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&directory).map_err(|_| "无法整理加密登录记录")? {
            let path = entry.map_err(|_| "无法整理加密登录记录")?.path();
            let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            let Some(revision) = name
                .strip_prefix("session-")
                .and_then(|v| v.strip_suffix(".json"))
                .and_then(|v| v.parse::<u64>().ok())
            else {
                continue;
            };
            if b.session_baseline
                .as_ref()
                .is_some_and(|o| o.revision == revision)
                || b.session_checkpoint
                    .as_ref()
                    .is_some_and(|o| o.revision == revision)
                || b.pending_session
                    .as_ref()
                    .is_some_and(|o| o.revision == revision)
            {
                continue;
            }
            if path.is_file() {
                std::fs::remove_file(&path).map_err(|_| "无法整理加密登录记录")?;
            }
        }
    }
    Ok(())
}
fn snapshot(c: &Connection, o: &Object) -> Result<Snapshot, String> {
    let plain = sync::decrypt(c, o)?;
    Snapshot::parse(serde_json::from_slice(&plain).map_err(|_| "登录快照无效")?)
}
fn hash(s: &Snapshot) -> Result<String, String> {
    Ok(format!("{:x}", Sha256::digest(s.bytes()?.as_slice())))
}
pub fn lease(c: &mut Connection, b: &mut Binding, takeover: bool) -> Result<(), String> {
    let v = c.request(
        Method::POST,
        &format!("/v1/objects/{}/lease", b.cloud_id),
        Some(&json!({"takeover":takeover})),
    )?;
    b.generation = v["generation"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("云端租约无效")?;
    Ok(())
}
fn conflict(
    c: &mut Connection,
    local: &str,
    b: &Binding,
    name: &str,
    field: &str,
    revision: u64,
) -> Result<(), String> {
    let a = c.account_id()?;
    let state = c.disk.accounts.get_mut(&a).unwrap();
    state
        .conflicts
        .retain(|x| x.local_id != local || x.kind != "session");
    state.conflicts.push(Conflict {
        local_id: local.into(),
        cloud_id: b.cloud_id.clone(),
        name: name.into(),
        kind: "session".into(),
        fields: vec![field.into()],
        remote_revision: revision,
    });
    Ok(())
}
fn stamp_path(
    path: &Path,
    relative: &Path,
    h: &mut Sha256,
    count: &mut usize,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    };
    profiles::guard_tree(path)?;
    let m = std::fs::metadata(path).map_err(|_| "无法读取登录数据状态")?;
    if m.is_dir() {
        let mut paths = std::fs::read_dir(path)
            .map_err(|_| "无法读取登录数据状态")?
            .map(|e| e.map(|v| v.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "无法读取登录数据状态")?;
        paths.sort();
        for p in paths {
            stamp_path(&p, &relative.join(p.file_name().unwrap()), h, count)?;
        }
    } else {
        *count += 1;
        if *count > 100000 {
            return Err("登录数据文件过多".into());
        };
        h.update(relative.to_string_lossy().as_bytes());
        h.update(m.len().to_le_bytes());
        h.update(
            m.modified()
                .map_err(|_| "无法读取登录数据时间")?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_le_bytes(),
        );
    }
    Ok(())
}
fn stamp(browser: &str, path: &Path) -> Result<String, String> {
    let mut h = Sha256::new();
    let files: &[&str] = if browser == "firefox" {
        &["cookies.sqlite", "cookies.sqlite-wal", "storage/default"]
    } else {
        &[
            "Default/Network/Cookies",
            "Default/Network/Cookies-wal",
            "Default/Cookies",
            "Default/Local Storage",
            "Default/IndexedDB",
        ]
    };
    for f in files {
        stamp_path(&path.join(f), Path::new(f), &mut h, &mut 0)?
    }
    Ok(format!("{:x}", h.finalize()))
}
fn apply(c: &Connection, p: &profiles::Profile, b: &mut Binding, o: &Object) -> Result<(), String> {
    let path = profiles::path(&c.root, p)?;
    let s = snapshot(c, o)?;
    let actual = browser_session::restore_profile(&p.browser, &path, &s)?;
    b.session_hash = Some(hash(&actual)?);
    b.session_baseline = Some(cache(c, o)?);
    b.pending_session = None;
    b.session_dirty = false;
    b.file_stamp = Some(stamp(&p.browser, &path)?);
    Ok(())
}
fn publish(
    c: &mut Connection,
    p: &profiles::Profile,
    b: &mut Binding,
    s: &Snapshot,
    base: u64,
    restore: bool,
) -> Result<(), String> {
    let bytes = s.bytes()?;
    let o = sync::upload(
        c,
        &b.cloud_id,
        "session",
        base,
        false,
        b.generation,
        restore,
        &bytes,
    )?;
    b.session_baseline = Some(cache(c, &o)?);
    b.session_hash = Some(hash(s)?);
    b.pending_session = None;
    b.session_dirty = false;
    b.file_stamp = Some(stamp(&p.browser, &profiles::path(&c.root, p)?)?);
    Ok(())
}
fn config_conflicted(c: &Connection, account: &str, id: &str) -> bool {
    c.disk.accounts[account]
        .conflicts
        .iter()
        .any(|x| x.local_id == id && x.kind != "session")
}
pub(super) fn one(
    c: &mut Connection,
    p: &profiles::Profile,
    b: &mut Binding,
    remote: Option<&Object>,
) -> Result<(), String> {
    reconcile(c, p, b, remote, false).map(|_| ())
}

enum SessionPreparation {
    Complete,
    LocalOnly,
}

fn reconcile(
    c: &mut Connection,
    p: &profiles::Profile,
    b: &mut Binding,
    remote: Option<&Object>,
    allow_local_launch: bool,
) -> Result<SessionPreparation, String> {
    let account = c.account_id()?;
    // Configuration choices also decide whether the profile and its session survive.
    if config_conflicted(c, &account, &p.id) {
        return Ok(SessionPreparation::Complete);
    }
    if let (Some(base), Some(remote)) = (
        b.session_baseline
            .as_ref()
            .or(b.session_checkpoint.as_ref()),
        remote,
    ) {
        sync::check_successor(base, remote)?;
        if remote.revision == base.revision {
            sync::check_successor(&cached(c, base)?, remote)?;
        }
    }
    let path = profiles::path(&c.root, p)?;
    let base_rev = b.session_baseline.as_ref().map(|o| o.revision).unwrap_or(0);
    let remote_rev = remote.map(|o| o.revision).unwrap_or(0);
    if profiles::running(&c.root, p)? {
        if remote_rev != base_rev {
            if let Some(o) = remote {
                b.pending_session = Some(cache(c, o)?)
            };
            conflict(c, &p.id, b, &p.name, "close_browser", remote_rev)?;
        }
        // A renewal never reacquires a lost lease. Fenced offline work is retained for resolution.
        if b.session_dirty && b.generation > 0 {
            let generation = b.generation;
            lease(c, b, false)?;
            if generation != b.generation {
                b.generation = 0;
                return Err("登录数据租约已到期，本机更改将保留为冲突".into());
            };
        }
        return Ok(SessionPreparation::Complete);
    }
    let file_stamp = stamp(&p.browser, &path)?;
    let changed = b.file_stamp.as_ref() != Some(&file_stamp) || b.session_dirty;
    if !changed && remote_rev == base_rev {
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .conflicts
            .retain(|x| x.local_id != p.id || x.kind != "session");
        return Ok(SessionPreparation::Complete);
    }
    let local = match browser_session::export_profile(&p.browser, &path) {
        Ok(local) => local,
        Err(error)
            if allow_local_launch
                && p.received_share.is_none()
                && error == browser_session::PARTITIONED_STORAGE_ERROR =>
        {
            // Migration capability is not a prerequisite for using an owner's
            // original profile. Keep both baselines and pending data untouched.
            c.disk.accounts.get_mut(&account).unwrap().last_error = Some(error);
            return Ok(SessionPreparation::LocalOnly);
        }
        Err(error) => return Err(error),
    };
    c.disk
        .accounts
        .get_mut(&account)
        .unwrap()
        .conflicts
        .retain(|x| x.local_id != p.id || x.kind != "session");
    let local_hash = hash(&local)?;
    let local_changed = b.session_hash.as_ref() != Some(&local_hash);
    if remote_rev == 0 && base_rev > 0 {
        conflict(c, &p.id, b, &p.name, "cloud_object_missing", 0)?;
        return Ok(SessionPreparation::Complete);
    }
    if remote_rev != base_rev {
        let o = remote.ok_or("云端登录记录不存在")?;
        let remote_snapshot = snapshot(c, o)?;
        let remote_hash = hash(&remote_snapshot)?;
        if local_hash == remote_hash {
            b.session_baseline = Some(cache(c, o)?);
            b.session_hash = Some(local_hash);
            b.file_stamp = Some(file_stamp);
            b.session_dirty = false;
            b.pending_session = None;
            return Ok(SessionPreparation::Complete);
        }
        // A live local profile may be awaiting a config deletion conflict, or may
        // have just been restored. A session tombstone requires an explicit choice.
        if o.deleted {
            b.pending_session = Some(cache(c, o)?);
            conflict(c, &p.id, b, &p.name, "deletion_vs_edit", remote_rev)?;
            return Ok(SessionPreparation::Complete);
        }
        if !local_changed || (base_rev == 0 && local.cookies.is_empty() && local.origins.is_empty())
        {
            apply(c, p, b, o)?;
            return Ok(SessionPreparation::Complete);
        }
        b.pending_session = Some(cache(c, o)?);
        conflict(c, &p.id, b, &p.name, "login_state_changed", remote_rev)?;
        return Ok(SessionPreparation::Complete);
    }
    if local_changed {
        lease(c, b, false)?;
        publish(
            c,
            p,
            b,
            &local,
            base_rev,
            b.session_baseline.as_ref().is_some_and(|o| o.deleted),
        )?;
    } else {
        b.file_stamp = Some(file_stamp);
        b.session_dirty = false;
    }
    Ok(SessionPreparation::Complete)
}
pub fn run(c: &mut Connection) -> Result<(), String> {
    let account = c.account_id()?;
    if !c.disk.accounts[&account].session_sync {
        return Ok(());
    };
    let list = sync::manifest(c)?;
    let bindings = c.disk.accounts[&account].bindings.clone();
    let all = profiles::load(&c.root)?;
    for (id, mut b) in bindings {
        if super::sharing::restricted(&c.root, &id)? {
            continue;
        }
        if c.disk.accounts[&account].detached.contains(&id) || config_conflicted(c, &account, &id) {
            continue;
        };
        let meta = list
            .iter()
            .find(|o| o.kind == "session" && o.id == b.cloud_id);
        if b.baseline.as_ref().is_some_and(|o| o.deleted) {
            if let Some(o) = meta.filter(|o| !o.deleted) {
                let result = (|| {
                    lease(c, &mut b, false)?;
                    let empty = Snapshot::empty().bytes()?;
                    sync::upload(
                        c,
                        &b.cloud_id,
                        "session",
                        o.revision,
                        true,
                        b.generation,
                        false,
                        &empty,
                    )
                })();
                let tombstone = match result {
                    Ok(o) => o,
                    Err(e)
                        if e.contains("lease_held")
                            || e.contains("lease_conflict")
                            || e.contains("revision_conflict") =>
                    {
                        conflict(
                            c,
                            &id,
                            &b,
                            "已删除副本",
                            "deletion_waiting_device",
                            o.revision,
                        )?;
                        c.disk.save(&c.root)?;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                b.session_baseline = Some(cache(c, &tombstone)?);
                b.session_hash = None;
                b.file_stamp = None;
                b.session_dirty = false;
                b.pending_session = None;
                c.disk
                    .accounts
                    .get_mut(&account)
                    .unwrap()
                    .bindings
                    .insert(id.clone(), b);
                c.disk.save(&c.root)?;
            }
            c.disk
                .accounts
                .get_mut(&account)
                .unwrap()
                .conflicts
                .retain(|x| x.local_id != id || x.kind != "session");
            c.disk.save(&c.root)?;
            continue;
        }
        let Some(p) = all.iter().find(|p| p.id == id) else {
            continue;
        };
        if p.deleted_at.is_some() {
            continue;
        };
        let remote = meta
            .map(|o| {
                if let Some(base) = &b.session_baseline {
                    if o.revision == base.revision && o.epoch == base.epoch {
                        return cached(c, base);
                    }
                };
                sync::fetch(c, &b.cloud_id, "session")
            })
            .transpose()?;
        if let Err(e) = one(c, &p, &mut b, remote.as_ref()) {
            if e.contains("config_conflict") {
                conflict(
                    c,
                    &id,
                    &b,
                    &p.name,
                    "config_changed",
                    remote.as_ref().map(|o| o.revision).unwrap_or(0),
                )?;
            } else if e.contains("lease_held")
                || e.contains("lease_conflict")
                || e.contains("revision_conflict")
            {
                conflict(
                    c,
                    &id,
                    &b,
                    &p.name,
                    "lease_or_revision_changed",
                    remote.as_ref().map(|o| o.revision).unwrap_or(0),
                )?;
            } else {
                c.disk
                    .accounts
                    .get_mut(&account)
                    .unwrap()
                    .bindings
                    .insert(id, b);
                c.disk.save(&c.root)?;
                return Err(e);
            }
        }
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(id, b);
        c.disk.save(&c.root)?;
    }
    Ok(())
}
pub fn before_launch(root: &Path, id: &str) -> Result<(), String> {
    if super::sharing::restricted(root, id)? {
        return Ok(());
    }
    if super::model::Disk::load(root)?.account.is_none() {
        return Ok(());
    }
    let mut c = Connection::load(root)?;
    prepare_launch(&mut c, id)
}
pub(super) fn prepare_launch(mut c: &mut Connection, id: &str) -> Result<(), String> {
    let root_path = c.root.clone();
    let root = root_path.as_path();
    let Ok(account) = c.account_id() else {
        return Ok(());
    };
    let state = &c.disk.accounts[&account];
    if !state.setup_complete
        || !state.session_sync
        || state.detached.contains(id)
        || !state.bindings.contains_key(id)
    {
        return Ok(());
    };
    let mut b = state.bindings[id].clone();
    if !state.auto_sync {
        b.session_dirty = true;
        b.generation = 0;
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(id.into(), b);
        return c.disk.save(root);
    }
    let p = profiles::get(root, id, false)?;
    let mut remote_revision = 0;
    let result = (|| {
        c.refresh_vault()?;
        c.unlocked_root()?;
        let list = sync::manifest(&mut c)?;
        let config = list
            .iter()
            .find(|o| o.kind == "config" && o.id == b.cloud_id);
        let config_current = match (b.baseline.as_ref(), config) {
            (Some(base), Some(remote)) => {
                base.revision == remote.revision
                    && base.epoch == remote.epoch
                    && base.deleted == remote.deleted
            }
            (None, None) => true,
            _ => false,
        };
        if !config_current || config_conflicted(c, &account, id) {
            return Err("云端配置已变化，请先同步副本配置后再打开浏览器".into());
        }
        let remote = list
            .iter()
            .find(|o| o.kind == "session" && o.id == b.cloud_id)
            .map(|o| sync::fetch(&mut c, &o.id, "session"))
            .transpose()?;
        remote_revision = remote.as_ref().map(|o| o.revision).unwrap_or(0);
        let local_only = matches!(
            reconcile(&mut c, &p, &mut b, remote.as_ref(), true)?,
            SessionPreparation::LocalOnly
        );
        if c.disk.accounts[&account]
            .conflicts
            .iter()
            .any(|x| x.local_id == id)
        {
            return Err("副本或登录状态有冲突，请在账号同步中选择保留本地、云端或两份".into());
        };
        if local_only {
            b.generation = 0;
            Ok(())
        } else {
            lease(&mut c, &mut b, false)
        }
    })();
    let offline = result.as_ref().err().is_some_and(|e| {
        e == "无法连接账号服务，请检查网络" || e == "云端响应中断" || e.contains("unauthorized")
    });
    if let Err(e) = &result {
        if offline {
            b.generation = 0;
            c.disk.accounts.get_mut(&account).unwrap().last_error = Some(e.clone());
        } else if e.contains("config_conflict") {
            conflict(&mut c, id, &b, &p.name, "config_changed", remote_revision)?;
        } else if e.contains("lease_held") {
            conflict(&mut c, id, &b, &p.name, "lease_held", remote_revision)?;
        }
    }
    b.session_dirty = true;
    c.disk
        .accounts
        .get_mut(&account)
        .unwrap()
        .bindings
        .insert(id.into(), b);
    c.disk.save(root)?;
    if offline {
        Ok(())
    } else {
        result
    }
}
pub(super) fn copy_with_session(
    c: &Connection,
    p: &profiles::Profile,
    draft: profiles::Draft,
    runtime: &profiles::Runtime,
) -> Result<profiles::Profile, String> {
    let source = profiles::path(&c.root, p)?;
    platform::ensure_profile_closed(&source)?;
    let snapshot = browser_session::export_profile(&p.browser, &source)?;
    let copied = profiles::create(&c.root, draft, Some(&p.id))?;
    if let Err(error) = browser_session::restore_profile(
        &copied.browser,
        &profiles::path(&c.root, &copied)?,
        &snapshot,
    ) {
        // This unpublished copy can be discarded; the source remains untouched.
        if profiles::remove(&c.root, &copied.id, false, runtime).is_ok() {
            let _ = profiles::remove(&c.root, &copied.id, true, runtime);
        }
        return Err(error);
    }
    Ok(copied)
}
pub fn resolve(
    c: &mut Connection,
    id: &str,
    choice: &str,
    runtime: &profiles::Runtime,
) -> Result<(), String> {
    super::sharing::editable(&c.root, id)?;
    let account = c.account_id()?;
    if config_conflicted(c, &account, id) {
        return Err("请先处理此副本的配置冲突，再选择登录状态".into());
    }
    let item = c.disk.accounts[&account]
        .conflicts
        .iter()
        .find(|x| x.local_id == id && x.kind == "session")
        .cloned()
        .ok_or("登录状态冲突已变化")?;
    if choice == "skip" {
        return Ok(());
    };
    if item.fields.iter().any(|f| f == "config_changed") {
        return Err("云端配置已变化，请先同步副本配置后再打开浏览器".into());
    }
    let p = profiles::get(&c.root, id, false)?;
    let path = profiles::path(&c.root, &p)?;
    platform::ensure_profile_closed(&path)?;
    let mut b = c.disk.accounts[&account].bindings[id].clone();
    if choice == "takeover" && item.fields.iter().any(|f| f == "lease_held") {
        lease(c, &mut b, true)?;
        let state = c.disk.accounts.get_mut(&account).unwrap();
        state.bindings.insert(id.into(), b);
        state
            .conflicts
            .retain(|x| x.local_id != id || x.kind != "session");
        return c.disk.save(&c.root);
    }
    let remote = sync::fetch(c, &b.cloud_id, "session")?;
    if remote.revision != item.remote_revision {
        return Err("云端版本再次变化，请重新同步".into());
    }
    match choice {
        "cloud" => apply(c, &p, &mut b, &remote)?,
        "local" => {
            let local = browser_session::export_profile(&p.browser, &path)?;
            lease(c, &mut b, true)?;
            publish(c, &p, &mut b, &local, remote.revision, remote.deleted)?;
        }
        "both" => {
            let mut draft = super::model::Document::export(&p)?.draft();
            draft.name = format!("{} (copy)", draft.name.chars().take(50).collect::<String>());
            let copied = copy_with_session(c, &p, draft, runtime)?;
            let state = c.disk.accounts.get_mut(&account).unwrap();
            state.known_local.insert(copied.id.clone());
            state
                .bindings
                .insert(copied.id, Binding::new(&uuid::Uuid::new_v4().to_string()));
            c.disk.save(&c.root)?;
            apply(c, &p, &mut b, &remote)?;
        }
        _ => return Err("冲突处理方式无效".into()),
    };
    let state = c.disk.accounts.get_mut(&account).unwrap();
    state.bindings.insert(id.into(), b);
    state
        .conflicts
        .retain(|x| x.local_id != id || x.kind != "session");
    let _ = runtime;
    c.disk.save(&c.root)
}

#[cfg(test)]
mod launch_tests {
    use super::super::{account::Session, client::Client, crypto, model::Account, vault};
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    struct Credentials(String, String);
    impl Drop for Credentials {
        fn drop(&mut self) {
            let _ = vault::remove(&format!("{}:device", self.0));
            let _ = vault::remove(&format!("{}:session", self.0));
            let _ = vault::remove(&vault::root_name(&self.0, &self.1, 1));
        }
    }

    fn fixture() -> (
        tempfile::TempDir,
        Connection,
        profiles::Profile,
        Credentials,
        std::thread::JoinHandle<()>,
    ) {
        let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let ns = format!("test-launch-{}", uuid::Uuid::new_v4());
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            email: "owner@example.invalid".into(),
            name: "Owner".into(),
            avatar: String::new(),
            google_linked: false,
            has_password: false,
            two_factor_enabled: false,
            recent_auth: false,
        };
        let credentials = Credentials(ns.clone(), account.id.clone());
        let mut c = Connection::test_connection(
            dir.path(),
            Client::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap(),
            &ns,
        )
        .unwrap();
        c.disk.account = Some(account.clone());
        c.save_session(Session {
            account_id: account.id.clone(),
            access_token: "synthetic-token".into(),
            refresh_token: "synthetic-refresh".into(),
            expires_at: chrono::Utc::now().timestamp() + 900,
        })
        .unwrap();
        let p = profiles::create(dir.path(), serde_json::from_value(json!({"name":"Owner original","browser":"firefox","proxy":{"mode":"system","host":"","port":0,"username":""}})).unwrap(), None).unwrap();
        let path = profiles::path(dir.path(), &p).unwrap();
        let partition = path.join(
            "storage/default/https+++example.invalid^partitionKey=%28https%2Cexample.invalid%29",
        );
        std::fs::create_dir_all(&partition).unwrap();
        std::fs::write(partition.join("sentinel"), b"local-session-preserved").unwrap();
        let key = crypto::random_key().unwrap();
        vault::save_root(&ns, &account.id, 1, &key).unwrap();
        let state = c.disk.accounts.entry(account.id.clone()).or_default();
        state.auto_sync = true;
        state.session_sync = true;
        state.setup_complete = true;
        state.epoch = 1;
        let mut binding = Binding::new(&uuid::Uuid::new_v4().to_string());
        binding.generation = 42;
        state.bindings.insert(p.id.clone(), binding);
        c.disk.save(dir.path()).unwrap();
        let info = json!({"epoch":1,"approved":true,
            "deviceEnvelope":crypto::device_seal(&*vault::decode_key(&c.device.public_key).unwrap(), &account.id, &c.device.id, 1, &key).unwrap(),
            "deviceProof":crypto::device_proof(&key, &account.id, &c.device.id, &c.device.public_key, 1)});
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            for (route, body) in [
                ("/v1/account", json!(account)),
                ("/v1/vault", info),
                ("/v1/objects", json!({"objects":[]})),
            ] {
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(std::time::Duration::from_millis(10))
                        }
                        Err(error) => panic!("missing mock request {route}: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                assert!(String::from_utf8(request)
                    .unwrap()
                    .starts_with(&format!("GET {route} ")));
                let body = body.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (dir, c, p, credentials, server)
    }

    #[test]
    fn owner_can_launch_with_partitioned_storage_without_publishing_or_replacing_sessions() {
        let (dir, mut c, p, _credentials, server) = fixture();
        assert!(super::super::sharing::before_launch(dir.path(), &p.id).is_ok());
        let result = prepare_launch(&mut c, &p.id);
        server.join().unwrap();
        assert!(result.is_ok(), "local launch blocked: {result:?}");
        let disk = super::super::model::Disk::load(dir.path()).unwrap();
        let state = &disk.accounts[&c.account_id().unwrap()];
        let binding = &state.bindings[&p.id];
        assert_eq!(binding.generation, 0);
        assert!(binding.session_dirty);
        assert!(binding.session_baseline.is_none());
        assert!(binding.pending_session.is_none());
        assert!(state.last_error.as_ref().unwrap().contains("分区网站存储"));
        let path = profiles::path(dir.path(), &p).unwrap();
        assert!(browser_session::export_profile("firefox", &path)
            .err()
            .unwrap()
            .contains("分区网站存储"));
        let account = c.account_id().unwrap();
        let mut binding = c.disk.accounts[&account].bindings[&p.id].clone();
        // Explicit synchronization must still fail rather than upload incomplete data.
        assert!(one(&mut c, &p, &mut binding, None)
            .unwrap_err()
            .contains("分区网站存储"));
        let mut received = p.clone();
        received.received_share = Some(uuid::Uuid::new_v4().to_string());
        assert!(reconcile(&mut c, &received, &mut binding, None, true)
            .err()
            .unwrap()
            .contains("分区网站存储"));
        // A subsequent local-only launch must retain a pending cloud revision.
        binding.pending_session = Some(Object {
            id: binding.cloud_id.clone(),
            kind: "session".into(),
            revision: 7,
            epoch: 1,
            deleted: false,
            payload: String::new(),
            writer: c.device.id.clone(),
            generation: 1,
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        assert!(matches!(
            reconcile(&mut c, &p, &mut binding, None, true).unwrap(),
            SessionPreparation::LocalOnly
        ));
        assert_eq!(binding.pending_session.as_ref().unwrap().revision, 7);
        // Other export failures must not become a local-only launch exemption.
        let other = profiles::create(
            dir.path(),
            super::super::model::Document::export(&p).unwrap().draft(),
            None,
        )
        .unwrap();
        let other_path = profiles::path(dir.path(), &other).unwrap().join("storage");
        std::fs::create_dir_all(&other_path).unwrap();
        std::fs::write(other_path.join("default"), b"unchanged").unwrap();
        assert!(reconcile(&mut c, &other, &mut binding, None, true)
            .err()
            .unwrap()
            .contains("无法读取网站存储"));
        assert_eq!(
            std::fs::read(other_path.join("default")).unwrap(),
            b"unchanged"
        );
        let partition = path.join("storage/default/https+++example.invalid^partitionKey=%28https%2Cexample.invalid%29/sentinel");
        assert_eq!(
            std::fs::read(partition).unwrap(),
            b"local-session-preserved"
        );
    }

    #[test]
    fn partitioned_storage_does_not_bypass_configuration_conflicts() {
        let (dir, mut c, p, _credentials, server) = fixture();
        let account = c.account_id().unwrap();
        let cloud_id = c.disk.accounts[&account].bindings[&p.id].cloud_id.clone();
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .conflicts
            .push(Conflict {
                local_id: p.id.clone(),
                cloud_id,
                name: p.name.clone(),
                kind: "config".into(),
                fields: vec!["name".into()],
                remote_revision: 1,
            });
        let result = prepare_launch(&mut c, &p.id);
        server.join().unwrap();
        assert!(result.unwrap_err().contains("云端配置已变化"));
        assert_eq!(
            super::super::model::Disk::load(dir.path())
                .unwrap()
                .accounts[&account]
                .conflicts
                .len(),
            1
        );
    }
}
