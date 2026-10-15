mod broker;
mod catalog;
mod operations;
pub(crate) mod policy;
mod remote;
mod stdio;
mod sync;
pub use broker::Runtime;
pub use stdio::run as run_stdio;
#[tauri::command]
pub async fn mcp_status(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    broker::snapshot(&app)
}
#[tauri::command]
pub async fn mcp_action(
    app: tauri::AppHandle,
    action: serde_json::Value,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || broker::action(&app, action))
        .await
        .map_err(|_| "MCP_ACTION_FAILED")?
}
pub fn start(app: tauri::AppHandle) {
    broker::start(app.clone());
    remote::start(app)
}
pub(crate) fn catalog_permission(tool: &str) -> Result<&'static str, String> {
    catalog::permission(tool).ok_or("UNKNOWN_TOOL".into())
}
