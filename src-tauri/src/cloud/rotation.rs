use super::{
    account::{required_string, Connection},
    crypto,
    model::Object,
    sync, vault,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Method;
use serde_json::{json, Value};
use zeroize::Zeroizing;
pub fn revoke(c: &mut Connection, id: &str) -> Result<Value, String> {
    if uuid::Uuid::parse_str(id).is_err() || id == c.device.id {
        return Err("请在其他已授权设备上撤销此设备".into());
    };
    c.refresh_vault()?;
    c.unlocked_root()?;
    let account = c.account_id()?;
    let epoch = c.disk.accounts[&account].epoch;
    let current_root = c.unlocked_root()?;
    let next = epoch.checked_add(1).ok_or("密钥版本超过限制")?;
    if !c.disk.accounts[&account].recovery_saved {
        return Err("请先保存当前恢复密钥".into());
    };
    let root = crypto::random_key()?;
    let recovery = crypto::random_key()?;
    let list = sync::manifest(c)?;
    let mut rotated = Vec::<Object>::new();
    let mut bodies = Vec::new();
    let mut size = 0;
    for m in list {
        let old = sync::fetch(c, &m.id, &m.kind)?;
        let plain = sync::decrypt(c, &old)?;
        let mut o = old.clone();
        o.revision += 1;
        o.epoch = next;
        o.writer = c.device.id.clone();
        o.payload = serde_json::to_string(&crypto::seal(&root, &o.context(&account), &plain)?)
            .map_err(|_| "无法更新同步密文")?;
        size += o.payload.len();
        if size > 256 << 20 {
            return Err("同步数据超过密钥轮换限制".into());
        };
        bodies.push(json!({"id":o.id,"kind":o.kind,"baseRevision":old.revision,"payload":o.payload,"deleted":o.deleted,"generation":o.generation}));
        rotated.push(o);
    }
    let devices = c.request(Method::GET, "/v1/devices", None)?;
    let devices = devices["devices"].as_array().ok_or("设备列表无效")?;
    if !devices
        .iter()
        .any(|d| d["id"] == id && d["status"] != "revoked")
    {
        return Err("设备授权已变化，请刷新".into());
    };
    let mut envelopes = Vec::new();
    for d in devices
        .iter()
        .filter(|d| d["status"] == "approved" && d["id"] != id)
    {
        let device = required_string(d, "id")?;
        let public = required_string(d, "publicKey")?;
        if !crypto::verify_device_proof(
            &current_root,
            &account,
            &device,
            &public,
            epoch,
            d["deviceProof"].as_str().unwrap_or(""),
        ) {
            return Err("设备授权记录校验失败，取消密钥轮换".into());
        };
        if d["fingerprint"] != super::account::fingerprint(&account, &device, &public) {
            return Err("设备指纹校验失败".into());
        };
        envelopes.push(json!({"deviceProof":crypto::device_proof(&root,&account,&device,&public,next),"id":device,"envelope":crypto::device_seal(&*vault::decode_key(&public)?,&account,&device,next,&root)?}));
    }
    let body = json!({"baseEpoch":epoch,"revokeDevice":id,"recoveryEnvelope":crypto::recovery_seal(&recovery,&account,next,&root)?,"recoveryVerifier":B64.encode(crypto::recovery_verifier(&recovery)?),"devices":envelopes,"objects":bodies});
    // Crash-safe: save new keys before committing the remote transaction. Epoch only
    // advances locally after the server acknowledges the entire rotation.
    vault::stage_root(&c.ns, &account, next, &root)?;
    vault::save_pending(&c.ns, &account, next, &recovery)?;
    let response = c.request(Method::POST, "/v1/vault/rotate", Some(&body))?;
    if response["epoch"] != next {
        return Err("密钥轮换响应无效".into());
    };
    vault::save_root(&c.ns, &account, next, &root)?;
    vault::clear_staged(&c.ns, &account, next)?;
    let bindings = c.disk.accounts[&account].bindings.clone();
    for (local, mut b) in bindings {
        for o in rotated.iter().filter(|o| o.id == b.cloud_id) {
            if o.kind == "config" {
                if b.baseline
                    .as_ref()
                    .is_some_and(|base| base.revision + 1 == o.revision)
                {
                    b.baseline = Some(o.clone())
                }
            } else if b
                .session_baseline
                .as_ref()
                .is_some_and(|base| base.revision + 1 == o.revision)
            {
                b.session_baseline = Some(super::session_sync::cache(c, o)?)
            };
        }
        b.generation = 0;
        c.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(local, b);
    }
    let state = c.disk.accounts.get_mut(&account).unwrap();
    state.epoch = next;
    state.recovery_saved = false;
    c.disk.save(&c.root)?;
    super::session_sync::prune_cache(c)?;
    let code = Zeroizing::new(vault::recovery_code(&recovery).to_string());
    Ok(json!({"recoveryCode":code.as_str()}))
}
