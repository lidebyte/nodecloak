//! Discover only a loopback listener owned by this exact browser profile.
use super::*;
use std::time::{Duration, Instant};

#[cfg(windows)]
fn owned_listener(port: u16, pids: &[u32]) -> bool {
    use windows_sys::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::AF_INET};
    if pids.is_empty() || port == 0 {
        return false;
    }
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    for _ in 0..3 {
        if size < 4 || size > 16 * 1024 * 1024 {
            return false;
        }
        let mut buffer = vec![0u32; (size as usize + 3) / 4];
        let capacity = buffer.len() * 4;
        let result = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut size,
                0,
                AF_INET as u32,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if result == 122 {
            continue;
        }
        if result != 0 {
            return false;
        }
        let count = buffer[0] as usize;
        let row_size = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
        if count > (capacity - 4) / row_size {
            return false;
        }
        for index in 0..count {
            let row = unsafe {
                std::ptr::read_unaligned(
                    buffer
                        .as_ptr()
                        .cast::<u8>()
                        .add(4 + index * row_size)
                        .cast::<MIB_TCPROW_OWNER_PID>(),
                )
            };
            if row.dwLocalAddr.to_ne_bytes() == [127, 0, 0, 1]
                && u16::from_be(row.dwLocalPort as u16) == port
                && pids.contains(&row.dwOwningPid)
            {
                return true;
            }
        }
        return false;
    }
    false
}

#[cfg(windows)]
pub(super) fn discover(profile: &Path, port: u16) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(700))
        .build()
        .map_err(|_| "无法建立本地配置接口查询")?;
    while Instant::now() < deadline {
        let pids = profile_process_ids(profile)?;
        if owned_listener(port, &pids) {
            let value = tauri::async_runtime::block_on(async {
                let mut response = client
                    .get(format!("http://127.0.0.1:{port}/json/version"))
                    .send()
                    .await
                    .ok()?
                    .error_for_status()
                    .ok()?;
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.ok()? {
                    if bytes.len() + chunk.len() > 16384 {
                        return None;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            });
            if let Some(value) = value {
                if let Some(url) = value["webSocketDebuggerUrl"].as_str() {
                    if let Some(marker) =
                        manual_endpoint_marker(&format!("DevTools listening on {url}"), port)
                    {
                        // Recheck the listener after the request, never accept a
                        // response from a different process or another profile.
                        if owned_listener(port, &pids)
                            && profile_process_ids(profile)?
                                .iter()
                                .any(|pid| pids.contains(pid))
                        {
                            return Ok(marker);
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err("未发现归属此副本的本地配置接口；已核对浏览器进程及监听端口".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn listener_requires_exact_process_and_loopback_binding() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(owned_listener(port, &[std::process::id()]));
        assert!(!owned_listener(port, &[0]));
        assert!(!owned_listener(port, &[]));
        let wildcard = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        assert!(!owned_listener(
            wildcard.local_addr().unwrap().port(),
            &[std::process::id()]
        ));
    }
}
