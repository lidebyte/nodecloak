#[test]
#[ignore = "Starts installed browsers, interrupts their control socket and closes their windows"]
fn native_control_reconnect_and_normal_exit() {
    for engine in ["chrome", "edge", "firefox"].into_iter().filter(|e| {
        std::env::var("CLAUDE_DONE_TEST_BROWSER")
            .map(|s| s == *e)
            .unwrap_or(true)
    }) {
        let mut prefs = Preferences::default();
        prefs.regional.timezone_mode = "custom".into();
        prefs.regional.timezone = "Asia/Tokyo".into();
        prefs.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        let test_browser = browser(engine, prefs, "");
        let (tx, rx) = mpsc::channel();
        test_browser
            .control
            .sender
            .send(Request::Disconnect(tx))
            .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let log = test_browser.path.join("nodecloak-control.jsonl");
        let until = Instant::now() + Duration::from_secs(40);
        loop {
            let contents = std::fs::read_to_string(&log).unwrap_or_default();
            if contents.contains("\"event\":\"reconnected\"") {
                break;
            }
            let error = test_browser.control.error.lock().unwrap().clone();
            if error.is_some() && engine == "firefox" && contents.contains("recovery_exhausted") {
                assert!(contents.contains("\"event\":\"reconnect_attempt\""));
                assert!(contents.contains("\"attempt\":3"));
                assert!(contents.contains("recovery_exhausted"));
                println!("firefox: standalone BiDi resume unavailable; 3 automatic attempts and diagnostic retained: {error:?}");
                break;
            }
            if engine != "firefox" {
                assert!(error.is_none(), "{engine}: {error:?}; {contents}");
            }
            assert!(
                Instant::now() < until,
                "{engine}: recovery timed out: {contents}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        if !test_browser.control.alive.load(Ordering::SeqCst) {
            // Verify normal close separately even on Firefox versions that cannot rejoin.
            drop(test_browser);
            let normal = browser(engine, Preferences::default(), "");
            let (tx, rx) = mpsc::channel();
            normal
                .control
                .sender
                .send(Request::ExitBrowser(tx))
                .unwrap();
            rx.recv_timeout(Duration::from_secs(20)).unwrap();
            std::thread::sleep(Duration::from_secs(3));
            assert!(normal.control.error.lock().unwrap().is_none());
            continue;
        }
        let grant = crate::mcp::policy::Grant {
            id: "reconnect-check".into(),
            name: "Test".into(),
            profile_ids: vec!["test".into()],
            permissions: vec!["read".into(), "page_read".into()],
            origins: vec![],
            expires_at: chrono::Utc::now().timestamp() + 120,
            confirm_interactions: false,
            file_roots: vec![],
            revoked: false,
            token_hash: String::new(),
        };
        let (tx, rx) = mpsc::channel();
        test_browser
            .control
            .sender
            .send(Request::Automation(
                "tabs_list".into(),
                json!({"profile_id":"test"}),
                grant,
                Arc::new(AtomicBool::new(false)),
                tx,
            ))
            .unwrap();
        let tabs = rx.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
        assert!(
            !tabs["items"].as_array().unwrap().is_empty(),
            "{engine}: {tabs}"
        );
        let (tx, rx) = mpsc::channel();
        test_browser
            .control
            .sender
            .send(Request::ExitBrowser(tx))
            .unwrap();
        rx.recv_timeout(Duration::from_secs(20)).unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        while test_browser.control.alive.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            test_browser.control.error.lock().unwrap().is_none(),
            "{engine}: normal close misreported"
        );
        println!("{engine}: actual socket interruption recovered and normal close stayed quiet");
    }
}
#[test]
#[ignore = "Compares native browser signals for ordinary and controlled startup; does not sign in to Google"]
fn native_google_login_compatibility_signals() {
    struct PlainProfile {
        path: PathBuf,
        _root: tempfile::TempDir,
    }
    impl Drop for PlainProfile {
        fn drop(&mut self) {
            let _ = platform::close_profile(&self.path);
            let _ = platform::force_close_profile(&self.path);
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let (send, receive) = mpsc::channel();
    let server = std::thread::spawn(move || {
        while !done.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut bytes = [0u8; 8192];
                let n = stream.read(&mut bytes).unwrap_or(0);
                let request = String::from_utf8_lossy(&bytes[..n]);
                let route = request
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/");
                let url = reqwest::Url::parse(&format!("http://{address}{route}")).unwrap();
                let body = if url.path() == "/signals" {
                    if let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "value") {
                        let _ = send.send(serde_json::from_str::<Value>(&value).unwrap());
                    }
                    "ok"
                } else {
                    "<html><title>Browser compatibility fixture</title><script>fetch('/signals?value='+encodeURIComponent(JSON.stringify({webdriver:navigator.webdriver,ua:navigator.userAgent})))</script></html>"
                };
                let _=write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
            } else {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    });
    for engine in ["chrome", "edge", "firefox"] {
        let controlled = browser(engine, Preferences::default(), "");
        controlled
            .control
            .open(format!("http://{address}/controlled"), false)
            .unwrap();
        let signals = receive.recv_timeout(Duration::from_secs(15)).unwrap();
        println!("{engine} controlled: {signals}");
        assert_eq!(signals["webdriver"], true);
        drop(controlled);
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let draft = Draft {
            name: "Native login compatibility".into(),
            browser: engine.into(),
            notes: String::new(),
            tags: vec![],
            proxy: ProxyConfig {
                mode: "direct".into(),
                ..Default::default()
            },
            password: None,
            clear_password: false,
            preferences: Preferences::default(),
        };
        let source_id = format!("legacy-{engine}");
        let p = profiles::create(&canonical, draft, Some(&source_id)).unwrap();
        assert!(!crate::regional::needs_control_for(engine, &p.preferences));
        profiles::apply_preferences(&canonical, &p).unwrap();
        let path = profiles::path(&canonical, &p).unwrap();
        let plain = PlainProfile { path, _root: root };
        platform::spawn_browser_with_proxy(
            engine,
            &plain.path,
            &format!("http://{address}/ordinary"),
            "direct",
            None,
        )
        .unwrap();
        let signals = receive.recv_timeout(Duration::from_secs(15)).unwrap();
        println!("{engine} ordinary: {signals}");
        assert_eq!(signals["webdriver"], false);
        assert!(!plain.path.join("DevToolsActivePort").exists());
        assert!(!plain.path.join("WebDriverBiDiServer.json").exists());
    }
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
}
