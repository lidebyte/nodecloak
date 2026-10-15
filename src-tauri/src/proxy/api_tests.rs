use super::*;
use serde_json::json;
fn api_up(url: &str) -> Upstream {
    Upstream {
        config: serde_json::from_value(json!({"mode":"http","apiUrl":url})).unwrap(),
        password: String::new(),
    }
}
async fn provider(body: &str, status: &str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = body.to_owned();
    let status = status.to_owned();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = header(&mut stream).await;
        stream
            .write_all(
                format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    (
        format!("http://127.0.0.1:{port}/extract?token=synthetic-private-token"),
        server,
    )
}
#[tokio::test]
async fn api_proxy_valid_list_selects_a_single_endpoint_before_creating_relay() {
    let (url, server) = provider("192.0.2.8:8080\r\n198.51.100.9:9090\r\n", "200 OK").await;
    let relay = start(api_up(&url), 0)
        .await
        .expect("API IP:port should resolve without manual host fields");
    drop(relay);
    server.await.unwrap();
}
#[tokio::test]
async fn api_proxy_http200_error_is_rejected_as_provider_error_not_missing_host() {
    let (url, server) = provider("IP whitelist required", "200 OK").await;
    let error = match start(api_up(&url), 0).await {
        Ok(_) => panic!("error body accepted"),
        Err(e) => e,
    };
    assert!(error.contains("API"), "{error}");
    assert!(error.contains("IP whitelist required"), "{error}");
    assert!(!error.contains("synthetic-private-token"));
    server.await.unwrap();
}

#[test]
fn api_proxy_parser_rejects_any_non_ip_port_row_and_accepts_only_complete_lists() {
    let url = "https://provider.invalid/api?token=synthetic-private-token";
    for body in [
        " \r\n",
        "192.0.2.1:0",
        "192.0.2.1:65536",
        "999.1.1.1:80",
        "host.invalid:80",
        "http://192.0.2.1:80",
        "192.0.2.1:80:user:pass",
        "192.0.2.1:80\nquota exceeded",
        "{\"code\":200,\"data\":[\"192.0.2.1:80\"]}",
        "<html>log in</html>",
    ] {
        assert!(
            api::parse(body, url).is_err(),
            "malformed API response accepted"
        );
    }
    let endpoint = api::parse("\u{feff}\r\n [2001:db8::1]:1080 \r\n192.0.2.1:80\r\n", url).unwrap();
    assert_eq!(endpoint.host, "2001:db8::1");
    assert_eq!(endpoint.port, 1080);
}
#[tokio::test]
async fn api_proxy_http_status_and_query_credentials_are_not_leaked_in_error() {
    let (url, server) = provider(
        "{\"message\":\"invalid token synthetic-private-token\"}",
        "403 Forbidden",
    )
    .await;
    let error = match start(api_up(&url), 0).await {
        Ok(_) => panic!("status ignored"),
        Err(e) => e,
    };
    assert!(error.contains("403"));
    assert!(!error.contains("synthetic-private-token"));
    assert!(!error.contains(&url));
    server.await.unwrap();
}
#[tokio::test]
async fn api_proxy_does_not_follow_redirects_or_accept_oversized_responses() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target.local_addr().unwrap().port();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = header(&mut stream).await;
        stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/redirect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    });
    assert!(start(api_up(&format!("http://127.0.0.1:{port}/api")), 0)
        .await
        .err()
        .unwrap()
        .contains("302"));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), target.accept())
            .await
            .is_err()
    );
    server.await.unwrap();
    let (url, server) = provider(&"x".repeat(65537), "200 OK").await;
    assert!(start(api_up(&url), 0)
        .await
        .err()
        .unwrap()
        .contains("64 KiB"));
    let _ = server.await;
}
#[tokio::test]
async fn api_proxy_stall_is_bounded_and_does_not_start_a_relay() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(20)).await;
    });
    let start_time = std::time::Instant::now();
    let error = start(api_up(&format!("http://127.0.0.1:{port}/api")), 0)
        .await
        .err()
        .unwrap();
    assert!(error.contains("超时"));
    assert!(start_time.elapsed() < Duration::from_secs(13));
    server.abort();
}
#[tokio::test]
async fn api_proxy_uses_local_first_hop_for_extraction_and_fetches_once_per_relay() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_port = first.local_addr().unwrap().port();
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_port = second.local_addr().unwrap().port();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let first_server = tokio::spawn(async move {
        loop {
            let (mut client, _) = first.accept().await.unwrap();
            let observed = observed.clone();
            tokio::spawn(async move {
                let request = String::from_utf8(header(&mut client).await.unwrap()).unwrap();
                assert!(
                    !request.contains("Proxy-Authorization:"),
                    "profile auth leaked to local hop"
                );
                if request.starts_with("GET http://provider.invalid/extract") {
                    observed.fetch_add(1, Ordering::SeqCst);
                    let body = format!("127.0.0.1:{second_port}");
                    client.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                } else {
                    assert!(
                        request.starts_with(&format!("CONNECT 127.0.0.1:{second_port} HTTP/1.1"))
                    );
                    let mut remote = TcpStream::connect(("127.0.0.1", second_port))
                        .await
                        .unwrap();
                    client.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut remote).await;
                }
            });
        }
    });
    let second_server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = second.accept().await.unwrap();
            let request = String::from_utf8(header(&mut stream).await.unwrap()).unwrap();
            assert!(request.starts_with("CONNECT website.invalid:443 HTTP/1.1"));
            assert!(request.contains(&STANDARD.encode("user:secret")));
            stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await.unwrap();
            let mut bytes = [0; 4];
            stream.read_exact(&mut bytes).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        }
    });
    let mut up = api_up("http://provider.invalid/extract");
    up.config.username = "user".into();
    up.password = "secret".into();
    up.config.pre_proxy = Some(LocalProxy {
        mode: "http".into(),
        host: "127.0.0.1".into(),
        port: first_port,
    });
    let relay = start(up, 0).await.unwrap();
    assert_eq!(relay.endpoint.as_ref().unwrap().port, second_port);
    for _ in 0..2 {
        let mut client = TcpStream::connect(("127.0.0.1", relay.port)).await.unwrap();
        client
            .write_all(b"CONNECT website.invalid:443 HTTP/1.1\r\nHost: website.invalid\r\n\r\n")
            .await
            .unwrap();
        assert!(header(&mut client)
            .await
            .unwrap()
            .starts_with(b"HTTP/1.1 200"));
        client.write_all(b"ping").await.unwrap();
        let mut bytes = [0; 4];
        client.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"ping");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    second_server.await.unwrap();
    first_server.abort();
}
