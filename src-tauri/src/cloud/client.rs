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
    if status==413 && error=="request_too_large" {
        return "云端服务暂不支持此快照大小，请更新服务后重试；本机数据已保留".into();
    }
    let known = [
        "authorization_not_found",
        "invalid_scope",
        "job_not_active",
        "invalid_request",
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
        "recipient_not_found",
        "recipient_device_approval_required",
        "recipient_device_unavailable",
        "recipient_device_changed",
        "invalid_deadline",
        "share_quota_exceeded",
        "share_id_conflict",
        "share_expired",
        "share_inactive",
        "share_device_unavailable",
        "share_authorization_required",
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
            let transfer=data_transfer(&method,path);
            let mut request = self
                .http
                .request(method, format!("{}{path}", self.endpoint));
            if transfer {request=request.timeout(Duration::from_secs(180));}
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
                let limit=if transfer {super::session_limits::WIRE_BYTES} else {12<<20};
                if bytes.len() + chunk.len() > limit {
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
fn data_transfer(method:&Method,path:&str)->bool {
    let path=path.split('?').next().unwrap_or(path);
    (method==Method::POST && (path=="/v1/vault/rotate" || path=="/v1/shares"))
        || path.strip_prefix("/v1/objects/").is_some_and(|p|!p.is_empty()&&!p.contains('/'))
        || path.starts_with("/v1/shares/")&&path.ends_with("/payload")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_lists_and_lease_requests_keep_the_short_api_budget(){
        assert!(!data_transfer(&Method::GET,"/v1/shares"));
        assert!(data_transfer(&Method::POST,"/v1/shares"));
        assert!(!data_transfer(&Method::GET,"/v1/objects"));
        assert!(!data_transfer(&Method::POST,"/v1/objects/example/lease"));
    }
    #[test]
    fn encrypted_snapshot_responses_can_exceed_the_old_twelve_mib_limit() {
        use std::{io::{Read,Write},net::TcpListener};
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let server=std::thread::spawn(move||{
            let (mut socket,_)=listener.accept().unwrap();let mut request=Vec::new();let mut byte=[0];
            while !request.ends_with(b"\r\n\r\n") {socket.read_exact(&mut byte).unwrap();request.push(byte[0]);}
            let body=format!("{{\"payload\":\"{}\"}}","x".repeat(13<<20));
            let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
        });
        let client=Client::new(&format!("http://{address}"),true).unwrap();
        let result=client.call::<Value>(Method::GET,"/v1/objects/00000000-0000-4000-8000-000000000001?kind=session",None,None);
        server.join().unwrap();assert_eq!(result.expect("large encrypted downloads must be readable")["payload"].as_str().unwrap().len(),13<<20);
    }
    #[test]
    fn sharing_device_unavailable_error_keeps_actionable_code_without_response_secrets() {
        let error = api_error(
            409,
            r#"{"error":"recipient_device_unavailable","email":"private@example.com","token":"secret-access-token"}"#,
        );
        assert!(error.contains("recipient_device_unavailable"));
        assert!(!error.contains("private@example.com"));
        assert!(!error.contains("secret-access-token"));
    }
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
