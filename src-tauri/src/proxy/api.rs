use super::{ProxyEndpoint, Upstream};
use reqwest::Url;
use std::time::Duration;
const MAX_BODY: usize = 65536;

pub(super) fn validate_url(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 4096
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("请填写有效的代理 API 地址".into());
    }
    let url = Url::parse(value).map_err(|_| "请填写有效的代理 API 地址")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("代理 API 仅支持不含用户名密码和片段的 HTTP / HTTPS 地址".into());
    }
    Ok(())
}
fn excerpt(text: &str, url: &str) -> String {
    let message = if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        ["message", "msg", "error"]
            .iter()
            .find_map(|key| value[key].as_str())
            .unwrap_or("接口返回内容不是 IP:端口 列表")
            .to_owned()
    } else if text.contains('<') {
        "接口返回了网页内容".into()
    } else {
        text.to_owned()
    };
    let mut message = message.replace(url, "[API]");
    if let Ok(parsed) = Url::parse(url) {
        let mut values: Vec<String> = parsed
            .query_pairs()
            .map(|(_, v)| v.into_owned())
            .filter(|v| !v.is_empty())
            .collect();
        values.extend(
            parsed
                .query()
                .unwrap_or("")
                .split('&')
                .filter_map(|p| p.split_once('=').map(|(_, v)| v.to_owned()))
                .filter(|v| !v.is_empty()),
        );
        values.sort_by_key(|v| std::cmp::Reverse(v.len()));
        values.dedup();
        for value in values {
            message = message.replace(&value, "[已隐藏]");
        }
    }
    message
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}
pub(super) fn parse(text: &str, url: &str) -> Result<ProxyEndpoint, String> {
    let mut endpoints = Vec::new();
    for line in text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let address = match line.parse::<std::net::SocketAddr>() {
            Ok(address) if address.port() != 0 => address,
            _ => {
                return Err(format!(
                    "代理 API 返回异常（应为 IP:端口）：{}",
                    excerpt(text, url)
                ))
            }
        };
        endpoints.push(ProxyEndpoint {
            host: address.ip().to_string(),
            port: address.port(),
        });
    }
    endpoints
        .into_iter()
        .next()
        .ok_or_else(|| "代理 API 返回为空，未提取到地址".into())
}
pub(super) async fn resolve(mut up: Upstream) -> Result<Upstream, String> {
    let Some(url) = up.config.api_url.clone() else {
        return Ok(up);
    };
    validate_url(&url)?;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(local) = &up.config.pre_proxy {
        let scheme = if local.mode == "socks5" {
            "socks5h"
        } else {
            &local.mode
        };
        let host = if local.host.contains(':') {
            format!("[{}]", local.host)
        } else {
            local.host.clone()
        };
        builder = builder.proxy(
            reqwest::Proxy::all(format!("{scheme}://{host}:{}", local.port))
                .map_err(|_| "本机代理地址无效")?,
        );
    }
    let client = builder.build().map_err(|_| "无法准备代理 API 请求")?;
    let mut response = client.get(&url).send().await.map_err(|e| {
        if e.is_timeout() {
            "代理 API 请求超时"
        } else {
            "代理 API 请求失败，请检查接口和本机网络"
        }
    })?;
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err("代理 API 响应超过 64 KiB".into());
    }
    let status = response.status();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "读取代理 API 响应失败或超时")?
    {
        if body.len() + chunk.len() > MAX_BODY {
            return Err("代理 API 响应超过 64 KiB".into());
        }
        body.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(body).map_err(|_| "代理 API 返回内容不是有效文本")?;
    if !status.is_success() {
        return Err(format!(
            "代理 API 请求失败（HTTP {}）：{}",
            status.as_u16(),
            excerpt(&text, &url)
        ));
    }
    let endpoint = parse(&text, &url)?;
    up.config.host = endpoint.host;
    up.config.port = endpoint.port;
    up.config.api_url = None;
    up.validate()?;
    Ok(up)
}
