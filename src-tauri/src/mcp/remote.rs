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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteRoute {
    pub route_id: String,
    pub local_grant_id: String,
    pub account_id: String,
    pub endpoint: String,
    #[serde(default)]
    expires_at: i64,
    #[serde(default)]
    revoke_attempts: u32,
    #[serde(default)]
    retry_after: i64,
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
            app.state::<profiles::Runtime>()
                .1
                .invalidate_downloads(Some(&client));
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
// The server's /v1/mcp/approve contract caps routes at 90 days. Legacy
// records have no creation time, so start that conservative bound when observed.
const MAX_SERVER_ROUTE_TTL: i64 = 90 * 86400;
fn collect_expired_routes(routes: &mut Vec<RemoteRoute>, now: i64) -> bool {
    let mut changed = false;
    routes.retain_mut(|route| {
        if route.expires_at == 0 {
            route.expires_at = now.saturating_add(MAX_SERVER_ROUTE_TTL);
            changed = true;
        }
        if route.expires_at <= now {
            changed = true;
            return false;
        }
        true
    });
    changed
}
fn route_snapshot(app: &tauri::AppHandle) -> Result<(bool, Vec<RemoteRoute>), String> {
    let runtime = app.state::<Runtime>();
    let mut state = runtime.0.lock().map_err(|_| "MCP_LOCK_FAILED")?;
    let before = state.config.remote_routes.clone();
    if collect_expired_routes(&mut state.config.remote_routes, broker::now()) {
        if let Err(e) = broker::persist(app, &state.config) {
            state.config.remote_routes = before;
            return Err(e);
        }
    }
    Ok((state.config.enabled, state.config.remote_routes.clone()))
}
fn check_route_capacity(
    routes: &[RemoteRoute],
    grants: &[super::policy::Grant],
    now: i64,
) -> Result<(), String> {
    let active = active_route_count(routes, grants, now);
    if active >= 100 {
        return Err("TOO_MANY_REMOTE_ROUTES".into());
    }
    // Pending acknowledgements are separate from the 100 live connections. Bound
    // this durable offline queue so repeated re-pairing cannot grow it forever.
    if routes.len().saturating_sub(active) >= 1000 {
        return Err("REMOTE_REVOCATIONS_PENDING: reconnect to finish revoking older connections before adding another".into());
    }
    Ok(())
}
fn route_has_live_grant(route: &RemoteRoute, grants: &[super::policy::Grant], now: i64) -> bool {
    grants
        .iter()
        .any(|g| g.id == route.local_grant_id && !g.revoked && g.expires_at > now)
}
fn active_route_count(routes: &[RemoteRoute], grants: &[super::policy::Grant], now: i64) -> usize {
    routes
        .iter()
        .filter(|r| route_has_live_grant(r, grants, now))
        .count()
}
// Pending server revocations are durable, but do not consume the live connection
// quota. Keep retrying with capped backoff until the server acknowledges them.
fn maintain_routes(
    routes: &mut Vec<RemoteRoute>,
    grants: &[super::policy::Grant],
    now: i64,
    mut revoke: impl FnMut(&str) -> Result<(), String>,
) -> Option<String> {
    let mut error = None;
    routes.retain_mut(|route| {
        if route_has_live_grant(route, grants, now) {
            return true;
        }
        if route.retry_after > now && route.retry_after.saturating_sub(now) <= 3600 {
            return true;
        }
        match revoke(&route.route_id) {
            Ok(()) => false,
            Err(e) => {
                route.revoke_attempts = route.revoke_attempts.saturating_add(1).min(11);
                let delay = (5_i64 << (route.revoke_attempts - 1)).min(3600);
                route.retry_after = now.saturating_add(delay);
                error.get_or_insert(e);
                true
            }
        }
    });
    error
}
fn retired_remote_grants(
    routes: &[RemoteRoute],
    requested: &[String],
    live: &[String],
) -> Vec<String> {
    routes
        .iter()
        .filter(|r| requested.contains(&r.route_id) && !live.contains(&r.route_id))
        .map(|r| r.local_grant_id.clone())
        .collect()
}
fn merge_route_updates(
    current: &mut Vec<RemoteRoute>,
    before: &[RemoteRoute],
    after: &[RemoteRoute],
) {
    current.retain_mut(|route| {
        if !before.iter().any(|r| r.route_id == route.route_id) {
            return true;
        }
        match after.iter().find(|r| r.route_id == route.route_id) {
            Some(updated) => {
                *route = updated.clone();
                true
            }
            None => false,
        }
    });
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
    route_snapshot(app)?;
    {
        let runtime = app.state::<Runtime>();
        let state = runtime.0.lock().map_err(|_| "MCP_LOCK_FAILED")?;
        check_route_capacity(
            &state.config.remote_routes,
            &state.config.grants,
            broker::now(),
        )?;
    }
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
        // Starting the requested TTL after receiving the response is an upper
        // bound independent of the server clock or time spent waiting to send.
        let expires_at = broker::now().saturating_add(remaining);
        let route = reply["route_id"]
            .as_str()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .ok_or("INVALID_REMOTE_ROUTE")?
            .to_string();
        let result = (|| {
            let runtime = app.state::<Runtime>();
            let mut state = runtime.0.lock().map_err(|_| "MCP_LOCK_FAILED")?;
            check_route_capacity(
                &state.config.remote_routes,
                &state.config.grants,
                broker::now(),
            )?;
            state.config.remote_routes.push(RemoteRoute {
                route_id: route.clone(),
                local_grant_id: id.clone(),
                account_id: account,
                endpoint: c.client.endpoint.clone(),
                expires_at,
                revoke_attempts: 0,
                retry_after: 0,
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
            let (enabled, routes) = match route_snapshot(&app) {
                Ok(value) => value,
                Err(error) => {
                    if let Ok(mut state) = app.state::<Runtime>().0.lock() {
                        state.remote_error = Some(error);
                    }
                    continue;
                }
            };
            if routes.is_empty() {
                if let Ok(mut state) = app.state::<Runtime>().0.lock() {
                    state.remote_active.clear();
                    state.remote_error = None;
                }
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
    let connection = (|| -> Result<_, String> {
        let c = Connection::load(root)?;
        let account = c.account_id()?;
        Ok((c, account))
    })();
    let (mut c, account) = match connection {
        Ok(value) => value,
        Err(e) => {
            for route in routes {
                app.state::<profiles::Runtime>()
                    .1
                    .invalidate_downloads(Some(&route.local_grant_id));
            }
            return Err(e);
        }
    };
    for route in routes
        .iter()
        .filter(|r| r.account_id != account || r.endpoint != c.client.endpoint)
    {
        app.state::<profiles::Runtime>()
            .1
            .invalidate_downloads(Some(&route.local_grant_id));
    }
    let matching: Vec<_> = routes
        .iter()
        .filter(|r| r.account_id == account && r.endpoint == c.client.endpoint)
        .collect();
    if matching.is_empty() {
        return Err("REMOTE_ACCOUNT_CHANGED".into());
    }
    let before: Vec<_> = matching.iter().map(|r| (**r).clone()).collect();
    let grants = app
        .state::<Runtime>()
        .0
        .lock()
        .map_err(|_| "MCP_LOCK_FAILED")?
        .config
        .grants
        .clone();
    for route in before
        .iter()
        .filter(|r| !enabled || !route_has_live_grant(r, &grants, broker::now()))
    {
        app.state::<profiles::Runtime>()
            .1
            .invalidate_downloads(Some(&route.local_grant_id));
    }
    let mut retained = before.clone();
    let cleanup_error = maintain_routes(&mut retained, &grants, broker::now(), |id| {
        c.request(
            Method::POST,
            &format!("/v1/mcp/routes/{id}/revoke"),
            Some(&json!({})),
        )
        .map(|_| ())
    });
    if retained != before {
        let runtime = app.state::<Runtime>();
        let mut state = runtime.0.lock().map_err(|_| "MCP_LOCK_FAILED")?;
        let previous = state.config.remote_routes.clone();
        // Approvals may append a route while the network request is in flight.
        merge_route_updates(&mut state.config.remote_routes, &before, &retained);
        if let Err(e) = broker::persist(app, &state.config) {
            state.config.remote_routes = previous;
            return Err(e);
        }
    }
    let active: Vec<_> = retained
        .iter()
        .filter(|r| enabled && broker::current_grant(app, &r.local_grant_id).is_ok())
        .map(|r| r.route_id.clone())
        .collect();
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
    // A successful relay response is authoritative for routes we actually
    // submitted. Preserve disabled routes, which were intentionally omitted.
    for id in retired_remote_grants(&before, &active, &live) {
        broker::action(app, json!({"kind":"revoke","id":id}))?;
    }
    for route in &before {
        if !live.contains(&route.route_id) {
            app.state::<profiles::Runtime>()
                .1
                .invalidate_downloads(Some(&route.local_grant_id));
        }
    }
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
    receipts.save(root)?;
    match cleanup_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
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
    fn old_route(id: &str, grant: &str) -> RemoteRoute {
        // Configurations written by 3.0.0 have no retry fields.
        serde_json::from_value(json!({"routeId":id,"localGrantId":grant,"accountId":"account","endpoint":"https://example.com"})).unwrap()
    }
    fn grant(id: &str, expires: i64, revoked: bool) -> super::super::policy::Grant {
        serde_json::from_value(json!({"id":id,"name":"fixture","profileIds":[],"permissions":["read"],"origins":[],"expiresAt":expires,"confirmInteractions":true,"fileRoots":[],"revoked":revoked,"tokenHash":""})).unwrap()
    }
    #[test]
    fn revoked_and_expired_routes_release_capacity() {
        let routes: Vec<_> = (0..100)
            .map(|i| old_route(&i.to_string(), "revoked"))
            .collect();
        let grants = vec![grant("revoked", 1000, true)];
        assert_eq!(active_route_count(&routes, &grants, 1), 0);
        let routes = vec![
            old_route("live", "live"),
            old_route("expired", "expired"),
            old_route("missing", "missing"),
        ];
        assert_eq!(
            active_route_count(
                &routes,
                &[grant("live", 10, false), grant("expired", 1, false)],
                1
            ),
            1
        );
    }
    #[test]
    fn successful_remote_revocation_is_forgotten_and_never_repeated() {
        let mut routes = vec![old_route("revoked", "revoked"), old_route("live", "live")];
        let grants = vec![grant("revoked", 1000, true), grant("live", 1000, false)];
        let mut requested = vec![];
        let _ = maintain_routes(&mut routes, &grants, 1, |id| {
            requested.push(id.to_owned());
            Ok(())
        });
        let _ = maintain_routes(&mut routes, &grants, 3, |id| {
            requested.push(id.to_owned());
            Ok(())
        });
        assert_eq!(requested, vec!["revoked"]);
        assert_eq!(
            routes
                .iter()
                .map(|r| r.route_id.as_str())
                .collect::<Vec<_>>(),
            vec!["live"]
        );
    }
    #[test]
    fn failed_revocation_survives_restart_and_does_not_retry_each_tick() {
        let mut routes = vec![old_route("revoked", "missing")];
        let mut attempts = 0;
        let _ = maintain_routes(&mut routes, &[], 10, |_| {
            attempts += 1;
            Err("offline".into())
        });
        routes = serde_json::from_slice(&serde_json::to_vec(&routes).unwrap()).unwrap();
        let _ = maintain_routes(&mut routes, &[], 12, |_| {
            attempts += 1;
            Err("offline".into())
        });
        assert_eq!(attempts, 1);
        assert_eq!(routes.len(), 1);
        let _ = maintain_routes(&mut routes, &[], 4000, |_| {
            attempts += 1;
            Ok(())
        });
        assert_eq!(attempts, 2);
        assert!(routes.is_empty());
    }
    #[test]
    fn revocation_backoff_is_capped_and_does_not_block_live_routes() {
        let mut routes = vec![
            old_route("failed", "missing"),
            old_route("expired", "expired"),
            old_route("live", "live"),
        ];
        let grants = vec![grant("expired", 10, false), grant("live", 100000, false)];
        let mut current = 10;
        let mut last_delay = 0;
        for _ in 0..16 {
            let mut called = vec![];
            let _ = maintain_routes(&mut routes, &grants, current, |id| {
                called.push(id.to_owned());
                if id == "failed" {
                    Err("offline".into())
                } else {
                    Ok(())
                }
            });
            assert!(!called.iter().any(|id| id == "live"));
            assert!(!routes.iter().any(|r| r.route_id == "expired"));
            let retry = routes
                .iter()
                .find(|r| r.route_id == "failed")
                .unwrap()
                .retry_after;
            last_delay = retry - current;
            assert!((5..=3600).contains(&last_delay));
            current = retry;
        }
        assert_eq!(last_delay, 3600);
        assert_eq!(active_route_count(&routes, &grants, current), 1);
    }
    #[test]
    fn cleanup_preserves_connections_approved_during_the_request() {
        let before = vec![old_route("revoked", "old"), old_route("live", "live")];
        let after = vec![old_route("live", "live")];
        let mut current = before.clone();
        current.push(old_route("just-approved", "new"));
        merge_route_updates(&mut current, &before, &after);
        assert_eq!(
            current
                .iter()
                .map(|r| r.route_id.as_str())
                .collect::<Vec<_>>(),
            vec!["live", "just-approved"]
        );
    }
    #[test]
    fn server_revocation_retires_only_routes_requested_from_this_connection() {
        let routes = vec![
            old_route("revoked", "old"),
            old_route("live", "live"),
            old_route("disabled", "disabled"),
        ];
        assert_eq!(
            retired_remote_grants(
                &routes,
                &["revoked".into(), "live".into()],
                &["live".into()]
            ),
            vec!["old"]
        );
        assert!(retired_remote_grants(&routes, &[], &[]).is_empty());
    }
    #[test]
    fn expired_route_collection_needs_no_account_and_preserves_legacy_upper_bound() {
        let mut expired = old_route("expired", "missing");
        expired.expires_at = 10;
        let mut current = old_route("current", "missing");
        current.expires_at = 30;
        let mut routes = vec![old_route("legacy", "missing"), expired, current];
        assert!(collect_expired_routes(&mut routes, 10));
        assert_eq!(
            routes
                .iter()
                .map(|r| r.route_id.as_str())
                .collect::<Vec<_>>(),
            vec!["legacy", "current"]
        );
        assert_eq!(routes[0].expires_at, 7_776_010);
        routes = serde_json::from_slice(&serde_json::to_vec(&routes).unwrap()).unwrap();
        assert!(!collect_expired_routes(&mut routes, 11));
        assert_eq!(
            routes[0].expires_at, 7_776_010,
            "restart must not continually extend legacy retention"
        );
        assert!(collect_expired_routes(&mut routes, 30));
        assert_eq!(routes.len(), 1);
        assert!(collect_expired_routes(&mut routes, 7_776_010));
        assert!(routes.is_empty());
    }
    #[test]
    fn pending_revocation_bound_is_separate_from_the_live_connection_quota() {
        let mut routes: Vec<_> = (0..999)
            .map(|i| old_route(&i.to_string(), "missing"))
            .collect();
        assert!(check_route_capacity(&routes, &[], 1).is_ok());
        routes.push(old_route("last", "missing"));
        assert!(check_route_capacity(&routes, &[], 1)
            .unwrap_err()
            .starts_with("REMOTE_REVOCATIONS_PENDING"));
        for route in &mut routes {
            route.expires_at = 1;
        }
        collect_expired_routes(&mut routes, 1);
        assert!(check_route_capacity(&routes, &[], 1).is_ok());
        let routes: Vec<_> = (0..100)
            .map(|i| old_route(&i.to_string(), "live"))
            .collect();
        assert_eq!(
            check_route_capacity(&routes, &[grant("live", 1000, false)], 1).unwrap_err(),
            "TOO_MANY_REMOTE_ROUTES"
        );
    }
    #[test]
    fn duplicate_receipts_never_dispatch_again_and_are_bound_to_route() {
        let mut receipts = Receipts::default();
        assert!(receipts.reserve("request", "route", 100).unwrap());
        assert!(!receipts.reserve("request", "route", 100).unwrap());
        assert!(receipts.reserve("request", "another-route", 100).is_err());
        assert!(receipts.items["request"].local_job.is_none());
    }
}
