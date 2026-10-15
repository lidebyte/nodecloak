use super::*;
use serde_json::json;

fn chained(local_mode: &str, port: u16, remote_mode: &str) -> Upstream {
    Upstream {
        config: serde_json::from_value(json!({"mode":remote_mode,"host":"second-hop.invalid","port":43210,"username":"remote-user",
            "preProxy":{"mode":local_mode,"host":"127.0.0.1","port":port}})).unwrap(),
        password:"remote-secret".into(),
    }
}
#[test]
fn chain_config_requires_custom_proxy_and_valid_local_endpoint() {
    let old: ProxyConfig = serde_json::from_value(json!({"mode":"direct"})).unwrap();
    assert!(old.validate().is_ok());
    for (mode, host, port, local_mode) in [
        ("direct", "127.0.0.1", 7890, "http"),
        ("system", "127.0.0.1", 7890, "socks5"),
        ("http", "http://router.invalid/path", 7890, "http"),
        ("http", "127.0.0.1", 0, "http"),
        ("http", "127.0.0.1", 7890, "direct"),
    ] {
        let config: ProxyConfig =
            serde_json::from_value(json!({"mode":mode,"host":"remote.invalid","port":8080,
            "preProxy":{"mode":local_mode,"host":host,"port":port}}))
            .unwrap();
        assert!(
            config.validate().is_err(),
            "invalid chain accepted: {mode}/{host}/{port}/{local_mode}"
        );
    }
}

#[test]
fn chain_accepts_router_and_lan_first_hops_without_accepting_urls_or_credentials() {
    for host in ["192.168.10.1", "10.0.0.1", "172.16.0.1", "fd00::1", "router.lan", "localhost", "::1"] {
        for mode in ["http", "https", "socks5"] {
            let mut up = chained(mode, 7890, "http");
            up.config.pre_proxy.as_mut().unwrap().host = host.into();
            assert!(up.validate().is_ok(), "rejected first hop: {host}/{mode}");
        }
    }
    for host in ["", "192.168.10.1:7890", "user@router.lan", "router.lan/path", "router\r\nHost: injected", "[fd00::1]", "bad..host"] {
        let mut up = chained("http", 7890, "http");
        up.config.pre_proxy.as_mut().unwrap().host = host.into();
        assert!(up.validate().is_err(), "accepted invalid first hop: {host:?}");
    }
}

// A local tunnel emulates the remote proxy over the very same connection.
// The unresolvable second-hop host proves it is not dialled directly.
#[tokio::test]
async fn chain_http_local_connects_remote_http_and_keeps_auth_on_second_hop() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let first = String::from_utf8(header(&mut stream).await.unwrap()).unwrap();
        assert!(first.starts_with("CONNECT second-hop.invalid:43210 HTTP/1.1"));
        assert!(!first.to_ascii_lowercase().contains("proxy-authorization"));
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        let second = String::from_utf8(header(&mut stream).await.unwrap()).unwrap();
        assert!(second.starts_with("CONNECT website.invalid:443 HTTP/1.1"));
        assert!(second.contains(&STANDARD.encode("remote-user:remote-secret")));
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let mut stream = tunnel(&chained("http", port, "http"), "website.invalid", 443)
        .await
        .unwrap();
    stream.write_all(b"ping").await.unwrap();
    let mut bytes = [0; 4];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"ping");
    server.await.unwrap();
}

async fn accept_hop(
    stream: &mut TcpStream,
    mode: &str,
    host: &str,
    port: u16,
    authenticated: bool,
) {
    if mode == "http" {
        let request = String::from_utf8(header(stream).await.unwrap()).unwrap();
        assert!(
            request.starts_with(&format!("CONNECT {host}:{port} HTTP/1.1")),
            "{request}"
        );
        assert_eq!(request.contains("Proxy-Authorization:"), authenticated);
        if authenticated {
            assert!(request.contains(&STANDARD.encode("remote-user:remote-secret")));
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
    } else {
        let mut greeting = [0; 3];
        stream.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting, [5, 1, if authenticated { 2 } else { 0 }]);
        stream
            .write_all(&[5, if authenticated { 2 } else { 0 }])
            .await
            .unwrap();
        if authenticated {
            assert_eq!(stream.read_u8().await.unwrap(), 1);
            let len = stream.read_u8().await.unwrap();
            let mut user = vec![0; len as usize];
            stream.read_exact(&mut user).await.unwrap();
            assert_eq!(user, b"remote-user");
            let len = stream.read_u8().await.unwrap();
            let mut password = vec![0; len as usize];
            stream.read_exact(&mut password).await.unwrap();
            assert_eq!(password, b"remote-secret");
            stream.write_all(&[1, 0]).await.unwrap();
        }
        let mut prefix = [0; 4];
        stream.read_exact(&mut prefix).await.unwrap();
        assert_eq!(prefix, [5, 1, 0, 3]);
        let len = stream.read_u8().await.unwrap();
        let mut domain = vec![0; len as usize];
        stream.read_exact(&mut domain).await.unwrap();
        assert_eq!(domain, host.as_bytes());
        assert_eq!(stream.read_u16().await.unwrap(), port);
        stream
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 1])
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn chain_mixed_protocols_keep_remote_dns_auth_and_tunnel_bytes() {
    for (first, second) in [("http", "socks5"), ("socks5", "http"), ("socks5", "socks5")] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            accept_hop(&mut stream, first, "second-hop.invalid", 43210, false).await;
            accept_hop(&mut stream, second, "website.invalid", 443, true).await;
            let mut bytes = [0; 4];
            stream.read_exact(&mut bytes).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        });
        let mut stream = tunnel(&chained(first, port, second), "website.invalid", 443)
            .await
            .unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"ping");
        server.await.unwrap();
    }
}
#[tokio::test]
async fn chain_plain_http_routes_through_both_hops_and_strips_client_proxy_auth() {
    for second in ["http", "socks5"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            accept_hop(&mut stream, "http", "second-hop.invalid", 43210, false).await;
            if second == "socks5" {
                accept_hop(&mut stream, second, "website.invalid", 80, true).await;
            }
            let request = String::from_utf8(header(&mut stream).await.unwrap()).unwrap();
            assert!(request.starts_with(if second == "http" {
                "GET http://website.invalid/path?q=1 HTTP/1.1"
            } else {
                "GET /path?q=1 HTTP/1.1"
            }));
            assert!(!request.contains("client-secret"));
            assert_eq!(
                request.contains(&STANDARD.encode("remote-user:remote-secret")),
                second == "http"
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .await
                .unwrap();
        });
        let relay = start(chained("http", port, second), 0).await.unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", relay.port)).await.unwrap();
        client.write_all(b"GET http://website.invalid/path?q=1 HTTP/1.1\r\nHost: website.invalid\r\nProxy-Authorization: client-secret\r\n\r\n").await.unwrap();
        assert!(header(&mut client)
            .await
            .unwrap()
            .starts_with(b"HTTP/1.1 200"));
        let mut body = [0; 2];
        client.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, b"ok");
        server.await.unwrap();
    }
}
#[tokio::test]
async fn chain_local_failure_never_dials_the_second_hop_directly() {
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_port = second.local_addr().unwrap().port();
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let mut up = chained("http", closed_port, "http");
    up.config.host = "127.0.0.1".into();
    up.config.port = second_port;
    let relay = start(up, 0).await.unwrap();
    let mut client = TcpStream::connect(("127.0.0.1", relay.port)).await.unwrap();
    client
        .write_all(b"CONNECT website.invalid:443 HTTP/1.1\r\nHost: website.invalid\r\n\r\n")
        .await
        .unwrap();
    assert!(header(&mut client)
        .await
        .unwrap()
        .starts_with(b"HTTP/1.1 502"));
    assert!(relay
        .failure
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .starts_with("前置代理连接失败"));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), second.accept())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn chain_rejected_first_or_second_hop_returns_failure_without_leaking_auth() {
    for reject_second in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            if reject_second {
                accept_hop(&mut stream, "http", "second-hop.invalid", 43210, false).await;
            }
            let request = String::from_utf8(header(&mut stream).await.unwrap()).unwrap();
            assert_eq!(request.contains("Proxy-Authorization:"), reject_second);
            stream
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .await
                .unwrap();
        });
        let error = match tunnel(&chained("http", port, "http"), "website.invalid", 443).await {
            Ok(_) => panic!("rejected hop accepted"),
            Err(e) => e,
        };
        assert_eq!(error.starts_with("前置代理连接失败"), !reject_second);
        assert!(!error.contains("remote-secret"));
        server.await.unwrap();
    }
}
#[tokio::test]
async fn chain_https_hops_keep_tls_handshake_instead_of_sending_plaintext_auth() {
    for first_tls in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            if !first_tls {
                accept_hop(&mut stream, "http", "second-hop.invalid", 43210, false).await;
            }
            assert_eq!(
                stream.read_u8().await.unwrap(),
                22,
                "expected native TLS handshake"
            );
            // Closing an untrusted/plaintext peer must fail, never downgrade TLS.
        });
        let result = tunnel(
            &chained(
                if first_tls { "https" } else { "http" },
                port,
                if first_tls { "http" } else { "https" },
            ),
            "website.invalid",
            443,
        )
        .await;
        assert!(result.is_err());
        server.await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Starts disposable Chrome, Edge and Firefox profiles through two local proxy hops"]
async fn native_chain_proxy_profiles_all_browsers() {
    native_proxy_profiles(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Starts disposable Chrome, Edge and Firefox with API extraction and local proxy chain"]
async fn native_api_proxy_profiles_all_browsers() {
    native_proxy_profiles(true).await;
}
async fn native_proxy_profiles(api_source: bool) {
    use crate::{platform, profiles};
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = platform::force_close_profile(&self.0);
        }
    }
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_port = second.local_addr().unwrap().port();
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_port = first.local_addr().unwrap().port();
    let (tx, mut reports) = tokio::sync::mpsc::unbounded_channel();
    let second_task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = second.accept().await else {
                break;
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let Ok(request) = header(&mut stream).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&request);
                let target = request
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("");
                if target.contains("/chain-beacon?") {
                    let _ = tx.send(target.to_owned());
                }
                let body = if target.contains("/chain-native?") {
                    "<!doctype html><title>Proxy chain test</title><script>fetch('/chain-beacon'+location.search)</script>"
                } else {
                    "ok"
                };
                let _=stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
            });
        }
    });
    let api_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed_calls = api_calls.clone();
    let first_task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = first.accept().await else {
                break;
            };
            let observed_calls = observed_calls.clone();
            tokio::spawn(async move {
                let Ok(request) = header(&mut stream).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&request);
                if api_source && request.starts_with("GET http://provider.invalid/extract-native") {
                    observed_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let body = format!("127.0.0.1:{second_port}");
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                    return;
                }
                let host = if api_source {
                    "127.0.0.1"
                } else {
                    "second-hop.invalid"
                };
                assert!(request.starts_with(&format!("CONNECT {host}:{second_port} HTTP/1.1")));
                assert!(!request.contains("Proxy-Authorization:"));
                let mut remote = TcpStream::connect(("127.0.0.1", second_port))
                    .await
                    .unwrap();
                stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut remote).await;
            });
        }
    });
    for (index, engine) in ["chrome", "edge", "firefox"].into_iter().enumerate() {
        let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let mut prefs = profiles::Preferences::default();
        let mut fingerprint = crate::fingerprint::fresh().unwrap();
        fingerprint.enabled = false;
        prefs.fingerprint = Some(fingerprint);
        prefs.startup_url = format!("http://chain-site.invalid/chain-native?engine={engine}");
        let mut proxy = json!({"mode":"http","host":"second-hop.invalid","port":second_port,
            "preProxy":{"mode":"http","host":"127.0.0.1","port":first_port}});
        if api_source {
            proxy["host"] = json!("");
            proxy["port"] = json!(0);
            proxy["apiUrl"] = json!("http://provider.invalid/extract-native");
        }
        let draft = profiles::Draft {
            name: format!("Chain fixture {engine}"),
            browser: engine.into(),
            notes: String::new(),
            tags: vec![],
            proxy: serde_json::from_value(proxy).unwrap(),
            password: None,
            clear_password: false,
            preferences: prefs,
        };
        let profile = profiles::create(root.path(), draft, None).unwrap();
        let path = profiles::path(root.path(), &profile).unwrap();
        let _cleanup = Cleanup(path.clone());
        let runtime = Arc::new(profiles::Runtime::default());
        let launch_runtime = runtime.clone();
        let launch_root = root.path().to_owned();
        let id = profile.id.clone();
        tokio::task::spawn_blocking(move || {
            crate::profile_commands::launch(&launch_root, &id, "startup", &launch_runtime)
        })
        .await
        .unwrap()
        .unwrap();
        let report = tokio::time::timeout(Duration::from_secs(35), reports.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            report.contains(&format!("/chain-beacon?engine={engine}")),
            "{engine}: {report}"
        );
        if api_source {
            assert_eq!(
                api_calls.load(std::sync::atomic::Ordering::SeqCst),
                index + 1,
                "API fetched more than once per launch"
            );
            let active = profiles::get(root.path(), &profile.id, false).unwrap();
            assert_eq!(
                active.active_proxy_endpoint.as_ref().unwrap().port,
                second_port
            );
        }
        println!("{engine}: API={api_source}; browser executed the page through local first hop and profile proxy");
        tokio::task::spawn_blocking(move || platform::force_close_profile(&path))
            .await
            .unwrap()
            .unwrap();
        runtime.1.stop(&profile.id);
        runtime.0.lock().unwrap().remove(&profile.id);
    }
    first_task.abort();
    second_task.abort();
}
