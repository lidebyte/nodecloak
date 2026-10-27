//! Recovery never replays a page operation: its effect may already have happened.
use super::*;
use std::{io::Write, net::SocketAddr};

pub(super) fn transport_error(error: &tungstenite::Error) -> String {
    match error {
        tungstenite::Error::Io(e) => format!(
            "I/O kind={:?}, os_code={:?}: {e}",
            e.kind(),
            e.raw_os_error()
        ),
        tungstenite::Error::Protocol(e) => format!("WebSocket protocol: {e}"),
        tungstenite::Error::Http(response) => {
            format!("WebSocket HTTP status={}", response.status().as_u16())
        }
        tungstenite::Error::ConnectionClosed => "WebSocket ConnectionClosed".into(),
        tungstenite::Error::AlreadyClosed => "WebSocket AlreadyClosed".into(),
        // Do not persist HTTP bodies, message bytes or close reasons: these can
        // contain page data or local session secrets.
        other => format!(
            "WebSocket {}",
            match other {
                tungstenite::Error::Http(_) => "HTTP handshake failed",
                tungstenite::Error::Utf8(_) => "invalid UTF-8",
                tungstenite::Error::Capacity(_) => "capacity exceeded",
                _ => "transport error",
            }
        ),
    }
}

pub(super) fn connect_socket(
    endpoint: &str,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let url = reqwest::Url::parse(endpoint).map_err(|_| "本地控制接口地址无效")?;
    if url.scheme() != "ws"
        || !matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
    {
        return Err("控制接口必须位于本机".into());
    }
    let address = SocketAddr::from(([127, 0, 0, 1], url.port().ok_or("控制接口端口无效")?));
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|e| format!("无法连接本地浏览器接口（{}）", transport_error(&e.into())))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let (mut socket, _) =
        tungstenite::client(endpoint, MaybeTlsStream::Plain(stream)).map_err(|e| match e {
            tungstenite::HandshakeError::Failure(error) => {
                format!("本地浏览器接口握手失败（{}）", transport_error(&error))
            }
            tungstenite::HandshakeError::Interrupted(_) => "本地浏览器接口握手未完成".into(),
        })?;
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
    }
    Ok(socket)
}

// One bounded JSONL file per profile, plus one rotated file. No arguments,
// URLs, form values, fingerprints, session ids or credentials are recorded.
pub(super) fn log(path: &Path, browser: &str, event: &str, attempt: usize, detail: &str) {
    let write = || -> Result<(), String> {
        let file = path.join("nodecloak-control.jsonl");
        let previous = path.join("nodecloak-control.previous.jsonl");
        crate::profiles::guard_tree(&file)?;
        crate::profiles::guard_tree(&previous)?;
        if std::fs::metadata(&file).is_ok_and(|m| m.len() > 1024 * 1024) {
            if previous.exists() {
                std::fs::remove_file(&previous).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&file, &previous).map_err(|e| e.to_string())?;
        }
        let record = json!({"time":chrono::Utc::now().to_rfc3339(), "browser":browser,
            "event":event,"attempt":attempt,"detail":detail.chars().take(2048).collect::<String>()});
        let mut output = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
            .map_err(|e| e.to_string())?;
        writeln!(output, "{record}").map_err(|e| e.to_string())
    };
    if let Err(error) = write() {
        eprintln!("browser control diagnostic write failed: {error}");
    }
}

// A browser closes its control socket before its last process exits. Wait for
// that transition before interpreting a disconnect as an error.
fn wait_for_exit(stop: &AtomicBool, delay: Duration, mut running: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + delay;
    loop {
        if stop.load(Ordering::SeqCst) || !running() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn recover(
    state: &mut BrowserControl,
    browser: &str,
    path: &Path,
    stop: &AtomicBool,
    cause: &str,
    recent: &mut VecDeque<Instant>,
) -> Result<bool, String> {
    log(path, browser, "disconnected", 0, cause);
    let running = || {
        platform::profile_process_ids(path)
            .map(|ids| !ids.is_empty())
            .unwrap_or(true)
    };
    if wait_for_exit(stop, Duration::from_millis(1200), running) {
        log(
            path,
            browser,
            "closed",
            0,
            "browser exited or controller stopped",
        );
        return Ok(false);
    }
    while recent
        .front()
        .is_some_and(|t| t.elapsed() > Duration::from_secs(60))
    {
        recent.pop_front();
    }
    if recent.len() >= 3 {
        return Err(format!("{cause}；一分钟内多次断开，已停止重连"));
    }
    recent.push_back(Instant::now());
    let mut last = cause.to_string();
    for (index, delay) in [250, 750, 1500].into_iter().enumerate() {
        if wait_for_exit(stop, Duration::from_millis(delay), running) {
            log(
                path,
                browser,
                "closed",
                index + 1,
                "browser exited or controller stopped",
            );
            return Ok(false);
        }
        log(
            path,
            browser,
            "reconnect_attempt",
            index + 1,
            "restoring original browser control settings",
        );
        match state.reconnect(browser, path) {
            Ok(()) => {
                log(
                    path,
                    browser,
                    "reconnected",
                    index + 1,
                    "original settings restored; interrupted page operations were not replayed",
                );
                return Ok(true);
            }
            Err(error) => {
                log(path, browser, "reconnect_failed", index + 1, &error);
                last = error;
            }
        }
    }
    if wait_for_exit(stop, Duration::from_millis(250), running) {
        return Ok(false);
    }
    Err(format!("{cause}；自动重连 3 次失败：{last}"))
}

impl BrowserControl {
    fn reconnect(&mut self, browser: &str, path: &Path) -> Result<(), String> {
        if self.firefox {
            // Rejoin the existing BiDi session; creating another session would
            // lose preloads, download ownership and input cleanup state.
            let endpoint = self
                .protocol
                .reconnect_endpoint
                .clone()
                .ok_or("Firefox 未返回可恢复的会话")?;
            let socket = match connect_socket(&endpoint) {
                Ok(socket) => socket,
                Err(error) => {
                    // Some Firefox builds retire a standalone session on close;
                    // others retain it but expose no route for rejoining it.
                    // Try a fresh session only after the old connection is gone.
                    let mut next = Self::initialize(
                        browser,
                        path,
                        self.region.clone(),
                        self.permission.clone(),
                        false,
                        "",
                        self.advanced.clone(),
                        self.fingerprint.clone(),
                        self.download_lease.is_some(),
                        self.plan,
                    )
                    .map_err(|new_error| format!("{error}；Firefox 会话恢复失败：{new_error}"))?;
                    next.configure()?;
                    if self.download_lease.is_some() {
                        next.download_policy_after_reconnect()?;
                        next.download_lease = self.download_lease.take();
                    }
                    *self = next;
                    self.invalidate_downloads(None)?;
                    return self.drain();
                }
            };
            self.protocol.socket = socket;
            self.protocol.events.clear();
            self.protocol.navigation = None;
            self.protocol.last_error = None;
            self.protocol
                .call("browsingContext.getTree", json!({"maxDepth":0}), None)?;
            self.configure()?;
        } else {
            // CDP session ids change after a disconnect. Reattach every live
            // page/frame/worker with the original region and fingerprint seed.
            let mut next = Self::initialize(
                browser,
                path,
                self.region.clone(),
                self.permission.clone(),
                false,
                "",
                self.advanced.clone(),
                self.fingerprint.clone(),
                self.download_lease.is_some(),
                self.plan,
            )?;
            // Reattachment may see only a remaining Incognito window. Never
            // redefine the capture boundary from whichever target arrives first.
            next.normal_context=self.normal_context.clone();
            next.session_capture_ready=self.session_capture_ready;
            next.configure()?;
            next.drain()?;
            // Missing events during disconnection make download state uncertain.
            // Retain ownership and deny new downloads until task tabs close.
            if self.download_lease.is_some() {
                next.download_policy_after_reconnect()?;
                next.download_lease = self.download_lease.take();
                next.invalidate_downloads(None)?;
            }
            next.restore_queue=std::mem::take(&mut self.restore_queue);
            *self = next;
        }
        // A reconnect can miss expiry/revoke events, so always enforce the lease
        // before accepting another MCP operation (also for a rejoined BiDi session).
        self.invalidate_downloads(None)?;
        self.drain()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_grace_suppresses_last_process_race_and_honors_stop() {
        let stop = AtomicBool::new(false);
        let mut count = 0;
        assert!(wait_for_exit(&stop, Duration::from_secs(1), || {
            count += 1;
            count < 3
        }));
        assert!(count >= 3);
        assert!(!wait_for_exit(&stop, Duration::ZERO, || true));
        stop.store(true, Ordering::SeqCst);
        assert!(wait_for_exit(&stop, Duration::from_secs(1), || true));
    }
    #[test]
    fn diagnostics_rotate_and_do_not_persist_close_reason_or_message_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap();
        let file = path.join("nodecloak-control.jsonl");
        std::fs::write(&file, vec![b' '; 1024 * 1024 + 1]).unwrap();
        log(
            &path,
            "chrome",
            "disconnected",
            1,
            "I/O ConnectionReset os_code=10054",
        );
        let value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(value["event"], "disconnected");
        assert!(value["detail"].as_str().unwrap().contains("10054"));
        assert!(path.join("nodecloak-control.previous.jsonl").exists());
    }
}
