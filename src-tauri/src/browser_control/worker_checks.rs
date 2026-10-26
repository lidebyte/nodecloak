#[test]
#[ignore = "Starts disposable Chrome and Edge profiles; exercises concurrent first-script worker protection"]
fn native_parallel_workers_preserve_settings_and_connection() {
    let fixture = Fixture::new();
    for engine in ["chrome", "edge"].into_iter().filter(|engine| {
        std::env::var("CLAUDE_DONE_TEST_BROWSER").map(|value| value == *engine).unwrap_or(true)
    }) {
        let mut prefs = Preferences::default();
        prefs.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        prefs.regional.timezone_mode = "custom".into();
        prefs.regional.timezone = "Asia/Tokyo".into();
        let browser = browser_for_purpose(engine, prefs, "", LaunchPurpose::Manual, |_| {});
        browser.control.open(fixture.url("/separation-workers"), false).unwrap();
        let report = fixture.reports.recv_timeout(Duration::from_secs(35)).unwrap();
        let workers = report["workers"].as_array().unwrap();
        assert_eq!(workers.len(), 16);
        for worker in workers {
            assert_eq!(worker["marker"], true, "{engine}: worker did not initialize: {report}");
            assert_eq!(worker["timezone"], "Asia/Tokyo", "{engine}: worker region lost");
        }
        assert!(browser.control.alive.load(Ordering::SeqCst));
        assert!(browser.control.error.lock().unwrap().is_none());
        let (send, receive) = mpsc::channel();
        browser.control.sender.send(Request::Disconnect(send)).unwrap();
        receive.recv_timeout(Duration::from_secs(3)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let log = std::fs::read_to_string(browser.path.join("nodecloak-control.jsonl")).unwrap_or_default();
            if log.contains("\"event\":\"reconnected\"") { break; }
            assert!(Instant::now() < deadline, "{engine}: live worker recovery failed: {log}");
            std::thread::sleep(Duration::from_millis(100));
        }
        browser.control.open(fixture.url("/separation-workers"), true).unwrap();
        let second = fixture.reports.recv_timeout(Duration::from_secs(35)).unwrap();
        assert!(second["workers"].as_array().unwrap().iter().all(|worker| worker["marker"] == true && worker["timezone"] == "Asia/Tokyo"));
        let (send, receive) = mpsc::channel();
        browser.control.sender.send(Request::ExitBrowser(send)).unwrap();
        receive.recv_timeout(Duration::from_secs(10)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while browser.control.alive.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!browser.control.alive.load(Ordering::SeqCst));
        assert!(browser.control.error.lock().unwrap().is_none(), "{engine}: normal shutdown misreported");
        println!("{engine}: 32 dedicated/shared workers retained fingerprint/timezone; live reconnect and normal exit passed");
    }
}
