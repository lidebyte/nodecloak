use reqwest::{Client as Http, Method, Url};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
pub struct Client {
    http: Http,
    pub endpoint: String,
}
pub fn validate_endpoint(s: &str, development: bool) -> Result<Url, String> {
    let u = Url::parse(s).map_err(|_| "账号服务地址无效")?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.host_str().is_none()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
        || u.scheme() != "https"
            && !(development
                && u.scheme() == "http"
                && matches!(u.host_str(), Some("127.0.0.1" | "localhost")))
    {
        return Err("账号服务必须使用可信 HTTPS 地址".into());
    };
    Ok(u)
}
pub fn api_error(status: u16, body: &str) -> String {
    let error = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_default();
    let known = [
        "device_approval_required",
        "unauthorized",
        "invalid_code",
        "try_later",
        "email_not_configured",
        "email_delivery_failed",
        "google_not_configured",
        "device_identity_mismatch",
        "vault_already_initialized",
        "vault_changed",
        "invalid_recovery_proof",
        "revision_conflict",
        "config_conflict",
        "lease_conflict",
        "lease_held",
        "quota_exceeded",
        "device_changed",
        "not_found",
        "invalid_oauth_flow",
        "google_cancelled",
        "account_link_conflict",
        "invalid_avatar",
        "invalid_name",
        "devices_changed",
        "invalid_rotation",
        "rotation_required",
        "device_limit",
        "password_length",
        "password_required",
        "invalid_credentials",
        "authentication_failed",
        "reauthentication_required",
        "two_factor_enabled",
    ];
    format!(
        "云端请求失败 ({status}): {}",
        if known.contains(&error.as_str()) {
            error.as_str()
        } else {
            "service_error"
        }
    )
}
impl Client {
    pub fn configured() -> Result<Self, String> {
        let endpoint = option_env!("NODECLOAK_API_URL").unwrap_or("https://nodecloak.com");
        Self::new(endpoint, cfg!(debug_assertions))
    }
    pub fn new(endpoint: &str, development: bool) -> Result<Self, String> {
        let u = validate_endpoint(endpoint, development)?;
        let http = Http::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "x-nodecloak-account-security",
                    reqwest::header::HeaderValue::from_static("1"),
                );
                headers
            })
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "无法初始化账号连接")?;
        Ok(Self {
            http,
            endpoint: u.as_str().trim_end_matches('/').into(),
        })
    }
    pub fn call<T: Serialize>(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&T>,
    ) -> Result<Value, String> {
        if !path.starts_with("/v1/") || path.contains("..") || path.contains('#') {
            return Err("无效云端接口".into());
        }
        tauri::async_runtime::block_on(async {
            let mut request = self
                .http
                .request(method, format!("{}{path}", self.endpoint));
            if let Some(t) = token {
                request = request.bearer_auth(t)
            };
            if let Some(b) = body {
                request = request.json(b)
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| "无法连接账号服务，请检查网络")?;
            let status = response.status();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| "云端响应中断")? {
                if bytes.len() + chunk.len() > 12 << 20 {
                    return Err("云端数据超出限制".into());
                };
                bytes.extend_from_slice(&chunk)
            }
            if !status.is_success() {
                return Err(api_error(
                    status.as_u16(),
                    std::str::from_utf8(&bytes).unwrap_or(""),
                ));
            }
            serde_json::from_slice(&bytes).map_err(|_| "云端响应格式无效".into())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_cannot_redirect_secrets_to_untrusted_protocol() {
        for s in [
            "https://user:pass@host",
            "http://example.com",
            "file:///etc/passwd",
            "https://example.com/?token=abc",
            "https://example.com/path",
            "https://example.com/#x",
        ] {
            assert!(validate_endpoint(s, false).is_err(), "{s}");
        }
        assert!(validate_endpoint("https://api.nodecloak.com", false).is_ok());
        assert!(validate_endpoint("http://127.0.0.1:8088", true).is_ok());
        assert!(validate_endpoint("http://127.0.0.1:8088", false).is_err());
    }
    #[test]
    fn cloud_error_does_not_include_response_secret_or_url() {
        let e = api_error(
            409,
            r#"{"error":"lease_conflict","payload":"private-cookie"}"#,
        );
        assert!(e.contains("lease_conflict"));
        assert!(!e.contains("private-cookie"));
        assert!(!api_error(500, "password=secret").contains("secret"));
    }
}
