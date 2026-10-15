//! MCP-only metadata projection and scoped execution. Never export decrypted documents.
use super::{broker, policy::Grant};
use crate::{
    cloud::{account::Connection, model::Document, sync},
    profiles, OperationLock,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, Manager};

fn selection(g: &Grant, a: &Value) -> Result<Vec<String>, String> {
    let mut ids = a["profile_ids"]
        .as_array()
        .ok_or("PROFILE_REQUIRED")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("PROFILE_REQUIRED".into())
        })
        .collect::<Result<Vec<String>, String>>()?;
    if ids.is_empty() || ids.len() > 100 {
        return Err("INVALID_ARGUMENTS".into());
    }
    ids.sort();
    ids.dedup();
    for id in &ids {
        g.authorize("sync", Some(id), broker::now())?;
    }
    if let Some(id) = a["profile_id"].as_str() {
        if !ids.iter().any(|x| x == id) {
            return Err("PROFILE_NOT_AUTHORIZED".into());
        }
    }
    if a["choice"] == "both" {
        g.authorize("manage", None, broker::now())?;
        if g.profile_ids.len() >= 200 {
            return Err("TOO_MANY_AUTHORIZED_PROFILES".into());
        }
    }
    Ok(ids)
}
fn fingerprint(client: &str, account: &str, issued: i64, versions: &Value) -> String {
    format!(
        "{issued}.{}",
        broker::hash(&json!([client, account, issued, versions]).to_string())
    )
}
fn issued(a: &Value) -> Result<i64, String> {
    if let Some(token) = a["preview_id"].as_str() {
        let n = token
            .split('.')
            .next()
            .and_then(|v| v.parse::<i64>().ok())
            .ok_or("SYNC_PREVIEW_CHANGED")?;
        if !(broker::now() - 600..=broker::now()).contains(&n) {
            return Err("SYNC_PREVIEW_EXPIRED".into());
        }
        Ok(n)
    } else {
        Ok(broker::now())
    }
}
fn preview(
    c: &mut Connection,
    g: &Grant,
    ids: &[String],
    issued: i64,
    cancel: &AtomicBool,
) -> Result<(Value, sync::VersionGuard), String> {
    c.unlocked_root()?;
    let account = c.account_id()?;
    let remote = sync::manifest(c)?;
    let mut rows = Vec::new();
    let mut versions = Vec::new();
    let mut cloud_ids = Vec::new();
    for id in ids {
        if cancel.load(Ordering::SeqCst) {
            return Err("CANCELLED".into());
        }
        crate::cloud::sharing::editable(&c.root, id)?;
        let p = profiles::get(&c.root, id, true)?;
        let b = c.disk.accounts[&account]
            .bindings
            .get(id)
            .cloned()
            .ok_or("SYNC_PROFILE_NOT_BOUND")?;
        if c.disk.accounts[&account].detached.contains(id) {
            return Err("SYNC_PROFILE_DETACHED".into());
        }
        let sessions = c.disk.accounts[&account].session_sync;
        cloud_ids.push(b.cloud_id.clone());
        let local =
            serde_json::to_value(Document::export(&p)?).map_err(|_| "SYNC_PREVIEW_FAILED")?;
        let base = b
            .baseline
            .as_ref()
            .map(|o| sync::document(c, o))
            .transpose()?
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| "SYNC_PREVIEW_FAILED")?;
        let ro = if remote
            .iter()
            .any(|o| o.id == b.cloud_id && o.kind == "config")
        {
            Some(sync::fetch(c, &b.cloud_id, "config")?)
        } else {
            None
        };
        let rv = ro
            .as_ref()
            .map(|o| sync::document(c, o))
            .transpose()?
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| "SYNC_PREVIEW_FAILED")?;
        let (action, fields) = match sync::decide(base.as_ref(), Some(&local), rv.as_ref()) {
            sync::Decision::Unchanged => ("unchanged", vec![]),
            sync::Decision::Upload(v) => (
                if v["deleted"] == true {
                    "delete_cloud"
                } else {
                    "upload"
                },
                vec![],
            ),
            sync::Decision::Apply(v) => (
                if v["deleted"] == true {
                    "delete_local"
                } else {
                    "download"
                },
                vec![],
            ),
            sync::Decision::Conflict(fields) => ("conflict", fields),
        };
        let running = profiles::running(&c.root, &p)?;
        let stamp = crate::cloud::session_sync::stamp(&p.browser, &profiles::path(&c.root, &p)?)?;
        let meta: Vec<Value> = remote.iter().filter(|o| o.id==b.cloud_id).map(|o|json!({"kind":o.kind,"revision":o.revision,"epoch":o.epoch,"writer":o.writer,"generation":o.generation,"deleted":o.deleted})).collect();
        versions.push(json!({"id":id,"config_hash":broker::hash(&local.to_string()),"binding":b,"remote":meta,"running":running,"stamp":stamp}));
        rows.push(json!({"profile_id":id,"name":p.name,"browser":p.browser,"config_action":action,"conflict_fields":fields,"running":running,"cloud_revision":ro.as_ref().map(|o|o.revision),"session_action":if !sessions {"disabled"} else if running {"wait_for_close"} else {"check_on_sync"}}));
    }
    let state = &c.disk.accounts[&account];
    let conflicts: Vec<_> = state
        .conflicts
        .iter()
        .filter(|x| ids.contains(&x.local_id))
        .collect();
    let version = json!({"profiles":versions,"conflicts":conflicts,"epoch":state.epoch,"setup":state.setup_complete,"sessions":state.session_sync,"recovery_saved":state.recovery_saved});
    Ok((
        json!({"preview_id":fingerprint(&g.id,&account,issued,&version),"profiles":rows,"conflicts":conflicts,"expires_at":issued+600}),
        sync::VersionGuard {
            ids: cloud_ids,
            objects: remote,
        },
    ))
}
pub fn execute(
    app: &tauri::AppHandle,
    g: &Grant,
    tool: &str,
    a: &Value,
    cancel: Arc<AtomicBool>,
) -> Result<Value, String> {
    let ids = selection(g, a)?;
    let root = &app.state::<crate::data_root::DataRoot>().0;
    let guard = app.state::<OperationLock>();
    let _lock = guard.0.lock().map_err(|_| "PROFILE_LOCK_FAILED")?;
    let current = broker::current_grant(app, &g.id)?;
    selection(&current, a)?;
    let bounded = super::remote::bounded_grant(app, &current, a)?;
    let mut c = Connection::load(root)?;
    let account = c.account_id()?;
    c.mcp_deadline = Some(bounded.expires_at);
    if tool == "sync_status" {
        let s = c.disk.accounts.get(&account).ok_or("SYNC_NOT_CONFIGURED")?;
        let bindings:Vec<_>=ids.iter().map(|id|json!({"profile_id":id,"bound":s.bindings.contains_key(id),"detached":s.detached.contains(id)})).collect();
        let conflicts: Vec<_> = s
            .conflicts
            .iter()
            .filter(|x| ids.contains(&x.local_id))
            .collect();
        return Ok(
            json!({"setup_complete":s.setup_complete,"bindings":bindings,"conflicts":conflicts,"last_sync":s.last_sync}),
        );
    }
    let (plan, versions) = preview(&mut c, &current, &ids, issued(a)?, &cancel)?;
    if tool == "sync_preview" {
        return Ok(plan);
    }
    if a["preview_id"] != plan["preview_id"] {
        return Err("SYNC_PREVIEW_CHANGED".into());
    }
    c.mcp_versions = Some(versions);
    if cancel.load(Ordering::SeqCst) {
        return Err("CANCELLED".into());
    }
    let runtime = app.state::<profiles::Runtime>();
    match tool {
        "sync_run" => sync::run_selected(&mut c, &runtime, Some(&ids), &cancel)?,
        "sync_resolve" => {
            let id = a["profile_id"].as_str().ok_or("PROFILE_REQUIRED")?;
            let choice = a["choice"].as_str().ok_or("INVALID_ARGUMENTS")?;
            // Keeping both introduces a new profile and must explicitly expand this client's grant.
            let before: Vec<_> = profiles::load(root)?.into_iter().map(|p| p.id).collect();
            match a["kind"].as_str() {
                Some("config") => sync::resolve(&mut c, id, choice, &runtime)?,
                Some("session") => {
                    crate::cloud::session_sync::resolve(&mut c, id, choice, &runtime)?
                }
                _ => return Err("INVALID_ARGUMENTS".into()),
            }
            for p in profiles::load(root)?
                .into_iter()
                .filter(|p| !before.contains(&p.id))
            {
                super::operations::attach_profile(app, &g.id, &p.id)?;
            }
        }
        _ => return Err("UNKNOWN_TOOL".into()),
    }
    let _ = app.emit("cloud-changed", ());
    let _ = app.emit("profiles-changed", ());
    let conflicts: Vec<_> = c.disk.accounts[&account]
        .conflicts
        .iter()
        .filter(|x| ids.contains(&x.local_id))
        .collect();
    Ok(json!({"profile_ids":ids,"conflicts":conflicts,"completed":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_bound_to_client_account_time_and_versions() {
        let a = fingerprint("one", "account", 1, &json!({"revision":1}));
        assert_ne!(a, fingerprint("two", "account", 1, &json!({"revision":1})));
        assert_ne!(a, fingerprint("one", "other", 1, &json!({"revision":1})));
        assert_ne!(a, fingerprint("one", "account", 2, &json!({"revision":1})));
        assert_ne!(a, fingerprint("one", "account", 1, &json!({"revision":2})));
    }
    #[test]
    fn selection_rejects_ungranted_profiles_and_normalizes_order() {
        let g = Grant {
            id: "g".into(),
            name: "test".into(),
            profile_ids: vec!["a".into(), "b".into()],
            permissions: vec!["read".into(), "sync".into()],
            origins: vec![],
            expires_at: broker::now() + 60,
            confirm_interactions: false,
            file_roots: vec![],
            revoked: false,
            token_hash: String::new(),
        };
        assert_eq!(
            selection(&g, &json!({"profile_ids":["b","a","a"]})).unwrap(),
            vec!["a", "b"]
        );
        assert!(selection(&g, &json!({"profile_ids":["other"]})).is_err());
        assert!(selection(&g, &json!({"profile_ids":["a"],"profile_id":"b"})).is_err());
        assert!(selection(&g, &json!({"profile_ids":["a"],"choice":"both"})).is_err());
    }
}
