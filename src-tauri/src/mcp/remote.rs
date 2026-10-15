use super::{
    broker::{self, Runtime},
    catalog,
};
use crate::{cloud::account::Connection, data_root, profiles};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteRoute {
    pub route_id: String,
    pub local_grant_id: String,
    pub account_id: String,
    pub endpoint: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub account: String,
    pub endpoint: String,
    pub route: String,
    pub deadline: i64,
}
pub fn bounded_grant(
    app: &tauri::AppHandle,
    g: &super::policy::Grant,
    args: &Value,
) -> Result<super::policy::Grant, String> {
    let mut bounded = g.clone();
    if let Some(value) = args.get("_remote_context") {
        let context: Context =
            serde_json::from_value(value.clone()).map_err(|_| "REMOTE_CONTEXT_INVALID")?;
        validate_context(app, &g.id, &context)?;
        bounded.expires_at = bounded.expires_at.min(context.deadline);
    }
    Ok(bounded)
}
fn validate_context(app: &tauri::AppHandle, client: &str, context: &Context) -> Result<(), String> {
    if broker::now() >= context.deadline {
        return Err("REMOTE_REQUEST_EXPIRED".into());
    }
    let authorized = app
        .state::<Runtime>()
        .0
        .lock()
        .map_err(|_| "MCP_LOCK_FAILED")?
        .config
        .remote_routes
        .iter()
        .any(|r| {
            r.route_id == context.route
                && r.local_grant_id == client
                && r.account_id == context.account
                && r.endpoint == context.endpoint
        });
    if !authorized {
        return Err("REMOTE_ROUTE_NOT_AUTHORIZED".into());
    }
    let c = Connection::load(&app.state::<data_root::DataRoot>().0)?;
    if c.account_id()? != context.account || c.client.endpoint != context.endpoint {
        return Err("REMOTE_ACCOUNT_CHANGED".into());
    }
    Ok(())
}
fn cancel_invalid_work(app: &tauri::AppHandle) {
    let contexts: Vec<_> = match app.state::<Runtime>().0.lock() {
        Ok(s) => s
            .jobs
            .iter()
            .filter(|j| matches!(j.state.as_str(), "queued" | "running" | "awaiting_approval"))
            .filter_map(|j| {
                j.remote_context
                    .clone()
                    .map(|c| (j.id.clone(), j.client.clone(), c))
            })
            .collect(),
        Err(_) => return,
    };
    for (id, client, context) in contexts {
        if let Err(e) = validate_context(app, &client, &context) {
            if let Ok(mut s) = app.state::<Runtime>().0.lock() {
                if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                    j.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    if j.state != "running" {
                        j.state = "cancelled".into();
                        j.error = Some(e);
                    }
                }
            }
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Receipts {
    items: BTreeMap<String, Receipt>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    route: String,
    local_job: Option<String>,
    expires: i64,
    finished: bool,
}
impl Receipts {
    fn reserve(&mut self, id: &str, route: &str, expires: i64) -> Result<bool, String> {
        if let Some(old) = self.items.get(id) {
            return if old.route == route {
                Ok(false)
            } else {
                Err("REMOTE_RECEIPT_MISMATCH".into())
            };
        }
        if self.items.len() >= 2000 {
            return Err("REMOTE_RECEIPT_LIMIT".into());
        }
        self.items.insert(
            id.into(),
            Receipt {
                route: route.into(),
                local_job: None,
                expires,
                finished: false,
            },
        );
        Ok(true)
    }
    fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("mcp-remote-receipts.json");
        profiles::guard_tree(&path)?;
        match std::fs::read(path) {
            Ok(data) if data.len() <= 1 << 20 => {
                serde_json::from_slice(&data).map_err(|_| "REMOTE_RECEIPTS_INVALID".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            _ => Err("REMOTE_RECEIPTS_INVALID".into()),
        }
    }
    fn save(&self, root: &Path) -> Result<(), String> {
        let path = root.join("mcp-remote-receipts.json");
        profiles::guard_tree(&path)?;
        let mut file =
            tempfile::NamedTempFile::new_in(root).map_err(|_| "REMOTE_RECEIPTS_WRITE_FAILED")?;
        serde_json::to_writer(&mut file, self).map_err(|_| "REMOTE_RECEIPTS_WRITE_FAILED")?;
        file.as_file()
            .sync_all()
            .map_err(|_| "REMOTE_RECEIPTS_WRITE_FAILED")?;
        file.persist(path)
            .map_err(|_| "REMOTE_RECEIPTS_WRITE_FAILED")?;
        Ok(())
    }
}
pub fn inspect(app: &tauri::AppHandle, code: &str) -> Result<Value, String> {
    let code = normalize_code(code)?;
    let mut c = Connection::load(&app.state::<data_root::DataRoot>().0)?;
    c.request(Method::GET, &format!("/v1/mcp/requests/{code}"), None)
}
fn normalize_code(code: &str) -> Result<String, String> {
    let value = code.replace(['-', ' '], "").to_uppercase();
    if value.len() != 12 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("INVALID_PAIRING_CODE".into());
    }
    Ok(value)
}
pub fn approve(app: &tauri::AppHandle, a: &Value) -> Result<Value, String> {
    let code = normalize_code(a["user_code"].as_str().ok_or("INVALID_PAIRING_CODE")?)?;
    let root = &app.state::<data_root::DataRoot>().0;
    let mut c = Connection::load(root)?;
    let account = c.account_id()?;
    let status = broker::action(app, json!({"kind":"grant","grant":a["grant"]}))?;
    let g = status["grants"]
        .as_array()
        .and_then(|v| v.last())
        .ok_or("GRANT_NOT_FOUND")?;
    let id = g["id"].as_str().ok_or("GRANT_NOT_FOUND")?.to_string();
    let outcome = (|| {
        let remaining = g["expiresAt"].as_i64().unwrap_or(0) - broker::now();
        let reply=c.request(Method::POST,"/v1/mcp/approve",Some(&json!({"user_code":code,"local_grant_id":id,"scopes":g["permissions"],"expires_in":remaining,"tools":catalog::tools()})))?;
        let route = reply["route_id"]
            .as_str()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .ok_or("INVALID_REMOTE_ROUTE")?
            .to_string();
        let result = (|| {
            let runtime = app.state::<Runtime>();
            let mut state = runtime.0.lock().map_err(|_| "MCP_LOCK_FAILED")?;
            if state.config.remote_routes.len() >= 100 {
                return Err("TOO_MANY_REMOTE_ROUTES".into());
            }
            state.config.remote_routes.push(RemoteRoute {
                route_id: route.clone(),
                local_grant_id: id.clone(),
                account_id: account,
                endpoint: c.client.endpoint.clone(),
            });
            if let Err(e) = broker::persist(app, &state.config) {
                state.config.remote_routes.pop();
                return Err(e);
            }
            Ok(())
        })();
        if let Err(e) = result {
            let _ = c.request(
                Method::POST,
                &format!("/v1/mcp/routes/{route}/revoke"),
                Some(&json!({})),
            );
            return Err(e);
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        let _ = broker::action(app, json!({"kind":"revoke","id":id}));
        return Err(e);
    }
    broker::snapshot(app)
}
pub fn start(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("nodecloak-mcp-relay".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(2));
            cancel_invalid_work(&app);
            let (enabled, routes) = match app.state::<Runtime>().0.lock() {
                Ok(s) => (s.config.enabled, s.config.remote_routes.clone()),
                Err(_) => continue,
            };
            if routes.is_empty() {
                continue;
            }
            let outcome = tick(&app, enabled, &routes);
            if let Ok(mut state) = app.state::<Runtime>().0.lock() {
                match outcome {
                    Ok(()) => {
                        state.remote_error = None;
                        state.remote_seen = broker::now()
                    }
                    Err(e) => {
                        state.remote_error = Some(e);
                    }
                }
            }
        })
        .expect("MCP relay thread");
}
fn tick(app: &tauri::AppHandle, enabled: bool, routes: &[RemoteRoute]) -> Result<(), String> {
    let root = &app.state::<data_root::DataRoot>().0;
    let mut c = Connection::load(root)?;
    let account = c.account_id()?;
    let matching: Vec<_> = routes
        .iter()
        .filter(|r| r.account_id == account && r.endpoint == c.client.endpoint)
        .collect();
    if matching.is_empty() {
        return Err("REMOTE_ACCOUNT_CHANGED".into());
    }
    let mut active = Vec::new();
    for r in &matching {
        if enabled && broker::current_grant(app, &r.local_grant_id).is_ok() {
            active.push(r.route_id.clone());
        } else if !app
            .state::<Runtime>()
            .0
            .lock()
            .map_err(|_| "MCP_LOCK_FAILED")?
            .config
            .grants
            .iter()
            .any(|g| g.id == r.local_grant_id && !g.revoked && g.expires_at > broker::now())
        {
            let _ = c.request(
                Method::POST,
                &format!("/v1/mcp/routes/{}/revoke", r.route_id),
                Some(&json!({})),
            );
        }
    }
    let mut receipts = Receipts::load(root)?;
    receipts
        .items
        .retain(|_, r| r.expires + 120 > broker::now());
    let response = c.request(
        Method::POST,
        "/v1/mcp/relay",
        Some(&json!({"routes":active})),
    )?;
    let live: Vec<String> = response["routes"]
        .as_array()
        .ok_or("REMOTE_REPLY_INVALID")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if let Ok(mut state) = app.state::<Runtime>().0.lock() {
        state.remote_active = live.clone();
    }
    for v in response["cancellations"]
        .as_array()
        .ok_or("REMOTE_REPLY_INVALID")?
    {
        let id = v["id"].as_str().unwrap_or("");
        if let Some(r) = receipts.items.get(id) {
            if r.route == v["route_id"].as_str().unwrap_or("") {
                if let Some(job) = &r.local_job {
                    if let Some(route) = matching.iter().find(|x| x.route_id == r.route) {
                        let _ = broker::local_request(
                            app,
                            &route.local_grant_id,
                            "jobs_cancel",
                            json!({"job_id":job}),
                        );
                    }
                }
            }
        }
    }
    for v in response["jobs"].as_array().ok_or("REMOTE_REPLY_INVALID")? {
        let id = v["id"]
            .as_str()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .ok_or("REMOTE_REPLY_INVALID")?;
        let route_id = v["route_id"].as_str().ok_or("REMOTE_REPLY_INVALID")?;
        let route = matching
            .iter()
            .find(|r| r.route_id == route_id && live.contains(&r.route_id))
            .ok_or("REMOTE_ROUTE_NOT_AUTHORIZED")?;
        let expires = v["expires_at"]
            .as_i64()
            .filter(|n| *n > broker::now() && *n <= broker::now() + 900)
            .ok_or("REMOTE_REQUEST_EXPIRED")?;
        if !receipts.reserve(id, route_id, expires)? {
            continue;
        }
        // Write-ahead receipt: any crash after this point is indeterminate, never a retry.
        receipts.save(root)?;
        if Connection::load(root)?.account_id()? != account {
            return Err("REMOTE_ACCOUNT_CHANGED".into());
        }
        let tool = v["tool"].as_str().ok_or("REMOTE_REPLY_INVALID")?;
        let context = Context {
            account: account.clone(),
            endpoint: c.client.endpoint.clone(),
            route: route_id.into(),
            deadline: expires,
        };
        let outcome = broker::remote_request(
            app,
            &route.local_grant_id,
            tool,
            v["arguments"].clone(),
            context,
        );
        if outcome["ok"] == true {
            receipts.items.get_mut(id).unwrap().local_job =
                outcome["result"]["id"].as_str().map(str::to_owned);
            receipts.save(root)?;
        } else {
            let code = outcome["error"]
                .as_str()
                .unwrap_or("REMOTE_DISPATCH_FAILED");
            post(&mut c, id, route_id, "failed", Value::Null, code)?;
            receipts.items.get_mut(id).unwrap().finished = true;
        }
    }
    for (id, receipt) in &mut receipts.items {
        if receipt.finished {
            continue;
        }
        let Some(route) = matching.iter().find(|r| r.route_id == receipt.route) else {
            continue;
        };
        if receipt.expires <= broker::now() {
            if let Some(job) = &receipt.local_job {
                let _ = broker::local_request(
                    app,
                    &route.local_grant_id,
                    "jobs_cancel",
                    json!({"job_id":job}),
                );
            }
            receipt.finished = true;
            continue;
        }
        if !live.contains(&receipt.route) {
            if let Some(job) = &receipt.local_job {
                let _ = broker::local_request(
                    app,
                    &route.local_grant_id,
                    "jobs_cancel",
                    json!({"job_id":job}),
                );
            }
            continue;
        }
        let Some(local) = &receipt.local_job else {
            post(
                &mut c,
                id,
                &receipt.route,
                "failed",
                Value::Null,
                "EXECUTION_INDETERMINATE",
            )?;
            receipt.finished = true;
            continue;
        };
        let result = broker::local_request(
            app,
            &route.local_grant_id,
            "jobs_get",
            json!({"job_id":local}),
        );
        if result["ok"] != true {
            post(
                &mut c,
                id,
                &receipt.route,
                "failed",
                Value::Null,
                "EXECUTION_INDETERMINATE",
            )?;
            receipt.finished = true;
            continue;
        }
        let job = &result["result"];
        let state = job["state"].as_str().ok_or("REMOTE_REPLY_INVALID")?;
        let state = if state == "queued" { "running" } else { state };
        match post(
            &mut c,
            id,
            &receipt.route,
            state,
            job["result"].clone(),
            job["error"].as_str().unwrap_or(""),
        ) {
            Ok(()) => receipt.finished = matches!(state, "completed" | "failed" | "cancelled"),
            Err(e) if e.contains("job_not_active") => {
                let _ = broker::local_request(
                    app,
                    &route.local_grant_id,
                    "jobs_cancel",
                    json!({"job_id":local}),
                );
                receipt.finished = true;
            }
            Err(e) => return Err(e),
        }
    }
    receipts.save(root)
}
fn post(
    c: &mut Connection,
    id: &str,
    route: &str,
    state: &str,
    result: Value,
    error: &str,
) -> Result<(), String> {
    c.request(Method::POST,&format!("/v1/mcp/relay/{id}"),Some(&json!({"route_id":route,"state":state,"result":result,"error":error.chars().take(200).collect::<String>()})))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_receipts_never_dispatch_again_and_are_bound_to_route() {
        let mut receipts = Receipts::default();
        assert!(receipts.reserve("request", "route", 100).unwrap());
        assert!(!receipts.reserve("request", "route", 100).unwrap());
        assert!(receipts.reserve("request", "another-route", 100).is_err());
        assert!(receipts.items["request"].local_job.is_none());
    }
}
