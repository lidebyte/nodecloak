#[test]
#[ignore = "Starts isolated browsers to verify MCP download policy ownership"]
fn native_mcp_download_clients_are_isolated() {
    let fixture = DownloadFixture::new();
    for engine in ["chrome", "edge", "firefox"].into_iter().filter(|e| {
        std::env::var("CLAUDE_DONE_TEST_BROWSER")
            .map(|s| s == *e)
            .unwrap_or(true)
    }) {
        let mut prefs = Preferences::default();
        prefs.startup_url = "about:blank".into();
        let files = tempfile::tempdir().unwrap();
        let manual = tempfile::tempdir().unwrap();
        let browser = browser_with_setup(engine, prefs, "", |path| {
            if engine == "firefox" {
                use std::io::Write;
                let mut user = std::fs::OpenOptions::new()
                    .append(true)
                    .open(path.join("user.js"))
                    .unwrap();
                writeln!(user,"\nuser_pref(\"browser.download.folderList\",2);\nuser_pref(\"browser.download.dir\",{});\nuser_pref(\"browser.download.useDownloadDir\",true);\nuser_pref(\"browser.helperApps.neverAsk.saveToDisk\",\"application/octet-stream\");",json!(manual.path().to_string_lossy())).unwrap();
            } else {
                let prefs = path.join("Default/Preferences");
                let mut value: Value =
                    serde_json::from_slice(&std::fs::read(&prefs).unwrap()).unwrap();
                value["download"] = json!({"default_directory":manual.path(),"prompt_for_download":false,"directory_upgrade":true});
                std::fs::write(prefs, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        });
        let a = crate::mcp::policy::Grant {
            id: uuid::Uuid::new_v4().to_string(),
            name: "A".into(),
            profile_ids: vec!["test".into()],
            permissions: vec![
                "read".into(),
                "page_read".into(),
                "page_write".into(),
                "files".into(),
            ],
            origins: vec![],
            expires_at: chrono::Utc::now().timestamp() + 600,
            confirm_interactions: false,
            file_roots: vec![files.path().to_string_lossy().into()],
            revoked: false,
            token_hash: String::new(),
        };
        let mut b = a.clone();
        b.id = uuid::Uuid::new_v4().to_string();
        b.name = "B".into();
        let call = |grant: &crate::mcp::policy::Grant, tool: &str, mut args: Value| {
            args["profile_id"] = json!("test");
            let (tx, rx) = mpsc::channel();
            browser
                .control
                .sender
                .send(Request::Automation(
                    tool.into(),
                    args,
                    grant.clone(),
                    Arc::new(AtomicBool::new(false)),
                    tx,
                ))
                .unwrap();
            rx.recv_timeout(Duration::from_secs(45)).unwrap()
        };
        let opened = call(&a, "tabs_open", json!({"url":format!("{}/",fixture.url)})).unwrap();
        let tab = opened["tab_id"].as_str().unwrap();
        call(&a, "page_wait", json!({"tab_id":tab,"text":"Ready"})).unwrap();
        let err = call(
            &b,
            "page_navigate",
            json!({"tab_id":tab,"url":"about:blank"}),
        )
        .expect_err("client B changed A's active download policy");
        assert!(err.contains("DOWNLOAD_CONTEXT_IN_USE"), "{engine}: {err}");
        let mut changed = a.clone();
        changed.file_roots.clear();
        changed.permissions.retain(|p| p != "files");
        assert!(call(
            &changed,
            "page_navigate",
            json!({"tab_id":tab,"url":"about:blank"})
        )
        .unwrap_err()
        .contains("DOWNLOAD_CONTEXT_IN_USE"));
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let result = call(&a, "downloads_list", json!({})).unwrap();
            if result["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["name"] == "delayed.txt" && f["bytes"] == 8)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{engine}: delayed download missing: {result}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            call(&b, "downloads_list", json!({})).unwrap()["files"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{engine}: A download visible to B"
        );
        call(
            &a,
            "page_navigate",
            json!({"tab_id":tab,"url":format!("{}/prompt",fixture.url)}),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while call(&a, "__preview_dialog", json!({"tab_id":tab})).is_err() {
            assert!(Instant::now() < deadline, "{engine}: prompt missing");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(call(&b, "page_dialog", json!({"tab_id":tab,"accept":true}))
            .unwrap_err()
            .contains("DOWNLOAD_CONTEXT_IN_USE"));
        call(&a, "page_dialog", json!({"tab_id":tab,"accept":false})).unwrap();
        call(&a, "tabs_close", json!({"tab_id":tab})).unwrap();
        let next = call(&b, "tabs_open", json!({"url":"about:blank"})).unwrap();
        assert!(
            next.get("navigation_error").is_none(),
            "{engine}: lease not released: {next}"
        );
        call(&b, "tabs_close", json!({"tab_id":next["tab_id"]})).unwrap();
        let denied = call(&changed, "tabs_open", json!({"url":"about:blank"})).unwrap();
        assert!(
            denied.get("navigation_error").is_none(),
            "{engine}: {denied}"
        );
        call(&changed, "tabs_close", json!({"tab_id":denied["tab_id"]})).unwrap();
        let manual_result = browser
            .control
            .open(format!("{}/file?manual", fixture.url), false);
        println!("{engine}: ordinary download navigation: {manual_result:?}");
        let deadline = Instant::now() + Duration::from_secs(15);
        while !std::fs::read(manual.path().join("delayed.txt")).is_ok_and(|v| v == b"fixture!") {
            assert!(Instant::now()<deadline,"{engine}: ordinary download did not restore to configured default folder; default files: {:?}; automation files: {:?}",std::fs::read_dir(manual.path()).unwrap().map(|e|e.unwrap().file_name()).collect::<Vec<_>>(),std::fs::read_dir(files.path()).unwrap().map(|e|e.unwrap().path()).collect::<Vec<_>>());
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(
            std::fs::read(manual.path().join("delayed.txt")).unwrap(),
            b"fixture!"
        );
        println!("{engine}: delayed downloads isolated; default manual downloads restored after task tabs closed");
    }
}

struct DownloadFixture {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl DownloadFixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    std::thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut bytes = [0; 4096];
                        let n = stream.read(&mut bytes).unwrap_or(0);
                        let request = String::from_utf8_lossy(&bytes[..n]);
                        let response = if request.starts_with("GET /file") {
                            std::thread::sleep(Duration::from_millis(1200));
                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=delayed.txt\r\nContent-Length: 8\r\nConnection: close\r\n\r\nfixture!".to_string()
                        } else {
                            let body = if request.starts_with("GET /prompt") {
                                "<html><body>Prompt<script>setTimeout(()=>confirm('Continue?'),100)</script></body></html>"
                            } else {
                                "<html><body>Ready<script>setTimeout(()=>{location.href='/file'},1500)</script></body></html>"
                            };
                            format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body)
                        };
                        let _ = stream.write_all(response.as_bytes());
                    });
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Self {
            url,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for DownloadFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.worker.take().unwrap().join();
    }
}
