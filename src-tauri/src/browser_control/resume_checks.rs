#[test]
#[ignore = "Starts disposable installed Chrome/Edge and restores native session state"]
fn native_session_resume_chromium_after_protection() {
    let fixture = Fixture::new();
    for engine in ["chrome", "edge"].into_iter().filter(|engine| {
        std::env::var("CLAUDE_DONE_TEST_BROWSER")
            .map(|v| v == *engine)
            .unwrap_or(true)
    }) {
        let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let mut preferences = Preferences::default();
        preferences.startup_url = "about:blank".into();
        preferences.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        preferences.regional.enabled = true;
        preferences.regional.timezone_mode = "custom".into();
        preferences.regional.timezone = "Asia/Tokyo".into();
        let draft: Draft = serde_json::from_value(json!({"name":"Session fixture","browser":engine,"proxy":{"mode":"direct","host":"","port":0,"username":""},"preferences":preferences})).unwrap();
        let p = profiles::create(root.path(), draft, None).unwrap();
        let path = profiles::path(root.path(), &p).unwrap();
        let runtime = profiles::Runtime::default();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = platform::force_close_profile(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        crate::profile_commands::launch_with_lookup(root.path(), &p.id, "startup", &runtime, "")
            .unwrap();
        runtime
            .1
            .open(&p.id, fixture.url("/separation?restore-session"), false)
            .unwrap();
        let first = fixture
            .reports
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        assert_eq!(first["main"]["timezone"], "Asia/Tokyo");
        assert_eq!(first["main"]["marker"], true);
        eprintln!("First fixture: {first}");
        if crate::session_restore::uses_tab_urls(engine) {
            let deadline = Instant::now() + Duration::from_secs(6);
            while crate::session_restore::read_tabs(&path).unwrap().is_empty() {
                assert!(
                    Instant::now() < deadline,
                    "normal session capture did not retain open tabs"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let (tx, rx) = mpsc::channel();
        runtime.1 .0.lock().unwrap()[&p.id]
            .sender
            .send(Request::ExitBrowser(tx))
            .unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        platform::wait_profile_closed(&path, Duration::from_secs(15)).unwrap();
        runtime.1.stop(&p.id);
        if crate::session_restore::uses_tab_urls(engine) {
            let saved = crate::session_restore::read_tabs(&path).unwrap();
            assert!(!saved.is_empty());
            platform::spawn_settings_browser(engine, &path, "direct", None).unwrap();
            let (control, _) = start_with_lookup(
                engine.into(), path.clone(), p.preferences.clone(),
                regional::initial(&p.preferences).unwrap(), String::new(), LaunchPurpose::Manual,
            ).unwrap();
            // Slow startup or delayed caller scheduling must not let the blank
            // helper overwrite the previous session before Resume is enqueued.
            std::thread::sleep(Duration::from_secs(3));
            assert_eq!(crate::session_restore::read_tabs(&path).unwrap(), saved);
            assert!(control.resume_previous().unwrap());
            runtime.1.insert(p.id.clone(), control).unwrap();
        } else {
            crate::profile_commands::launch_with_lookup(root.path(), &p.id, "startup", &runtime, "")
                .unwrap();
        }
        let restored = fixture
            .reports
            .recv_timeout(Duration::from_secs(20))
            .expect("last page was not restored");
        eprintln!("Restored fixture: {restored}");
        assert_eq!(restored["main"]["timezone"], "Asia/Tokyo");
        assert_eq!(
            restored["main"]["marker"], true,
            "restored page ran before fingerprint protection"
        );
        if !crate::session_restore::uses_tab_urls(engine) {
            assert_eq!(restored["main"]["draft"], "saved draft");
            assert_eq!(restored["main"]["scroll"], 700);
        }
        let (tx, rx) = mpsc::channel();
        runtime.1 .0.lock().unwrap()[&p.id]
            .sender
            .send(Request::ExitBrowser(tx))
            .unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        runtime.1.stop(&p.id);
        platform::close_profile(&path).unwrap();
        // MCP must start only at its requested destination, never replay a
        // human session's old origins without an automation request.
        crate::profile_commands::launch_for_automation(root.path(), &p.id, "startup", &runtime)
            .unwrap();
        assert!(runtime.1.automation_ready(&p.id));
        assert!(matches!(
            fixture.reports.recv_timeout(Duration::from_millis(800)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        let (tx, rx) = mpsc::channel();
        runtime.1 .0.lock().unwrap()[&p.id]
            .sender
            .send(Request::ExitBrowser(tx))
            .unwrap();
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        runtime.1.stop(&p.id);
        platform::close_profile(&path).unwrap();
    }
}

#[test]
#[ignore = "Starts disposable installed Firefox and restores its native saved session"]
fn native_session_resume_firefox_after_region_setup() {
    check_firefox_session_resume(true);
}
#[test]
#[ignore = "Starts disposable Firefox; saved pages must not opt a system-region profile into locale overrides"]
fn native_session_resume_firefox_system_settings_unchanged() {
    check_firefox_session_resume(false);
}
fn check_firefox_session_resume(custom: bool) {
    let fixture = Fixture::new();
    let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let mut preferences = Preferences::default();
    preferences.startup_url = fixture.url("/separation?restore-session&locale-field");
    if custom {
        preferences.regional.enabled = true;
        preferences.regional.timezone_mode = "custom".into();
        preferences.regional.timezone = "Asia/Tokyo".into();
    } else {
        preferences.language = "fr-FR,fr".into();
    }
    let mut fingerprint = crate::fingerprint::fresh().unwrap();
    fingerprint.enabled = false;
    preferences.fingerprint = Some(fingerprint);
    let draft:Draft=serde_json::from_value(json!({"name":"Firefox saved session","browser":"firefox","proxy":{"mode":"direct","host":"","port":0,"username":""},"preferences":preferences})).unwrap();
    let p = profiles::create(root.path(), draft, None).unwrap();
    let path = profiles::path(root.path(), &p).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = platform::force_close_profile(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let runtime = profiles::Runtime::default();
    crate::profile_commands::launch_with_lookup(root.path(), &p.id, "startup", &runtime, "")
        .unwrap();
    let first = fixture
        .reports
        .recv_timeout(Duration::from_secs(25))
        .unwrap();
    if custom {
        assert_eq!(first["main"]["timezone"], "Asia/Tokyo");
    }
    platform::close_profile(&path).unwrap();
    assert!(
        path.join("sessionstore.jsonlz4").exists(),
        "Firefox did not persist a native saved session"
    );
    crate::profile_commands::launch_with_lookup(root.path(), &p.id, "startup", &runtime, "")
        .unwrap();
    let restored = fixture
        .reports
        .recv_timeout(Duration::from_secs(25))
        .unwrap();
    eprintln!("Firefox restored fixture: {restored}");
    assert_eq!(restored["main"]["timezone"], first["main"]["timezone"]);
    assert_eq!(restored["main"]["locale"], first["main"]["locale"]);
    assert_eq!(restored["main"]["webdriver"], false);
    assert!(restored["main"]["cookie"]
        .as_str()
        .unwrap()
        .contains("nodecloak-session=restored"));
    assert_eq!(restored["main"]["draft"], "saved draft");
    assert_eq!(restored["main"]["scroll"], 700);
    assert_ne!(first["main"]["documentId"], restored["main"]["documentId"]);
    assert!(!path.join("nodecloak-session-restore").exists());
    platform::close_profile(&path).unwrap();
}

#[test]
#[ignore = "Uses existing registered NodeCloak native host read-only, installs the signed companion only in a disposable profile"]
fn native_session_resume_firefox_signed_protection() {
    let fixture = Fixture::new();
    let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let mut preferences = Preferences::default();
    preferences.fingerprint = Some(crate::fingerprint::fresh().unwrap());
    preferences.regional.enabled = true;
    preferences.regional.timezone_mode = "custom".into();
    preferences.regional.timezone = "Asia/Tokyo".into();
    let draft:Draft=serde_json::from_value(json!({"name":"Signed Firefox session","browser":"firefox","proxy":{"mode":"direct","host":"","port":0,"username":""},"preferences":preferences})).unwrap();
    let p = profiles::create(root.path(), draft, None).unwrap();
    let path = profiles::path(root.path(), &p).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = platform::force_close_profile(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    profiles::apply_preferences(root.path(), &p).unwrap();
    crate::firefox_extension::install_session_fixture(
        root.path(),
        &path,
        p.preferences.fingerprint.as_ref().unwrap(),
    )
    .unwrap();
    let url = fixture.url("/separation?restore-session");
    crate::firefox_region::launch(&path, &url, &p.preferences, "direct", None, "").unwrap();
    let first = fixture
        .reports
        .recv_timeout(Duration::from_secs(25))
        .unwrap();
    assert_eq!(first["main"]["marker"], true);
    platform::close_profile(&path).unwrap();
    let mut saved = crate::session_restore::FirefoxSession::park(&path)
        .unwrap()
        .unwrap();
    crate::firefox_region::launch_resuming(
        &path,
        &url,
        &p.preferences,
        "direct",
        None,
        "",
        &mut saved,
    )
    .unwrap();
    let report = fixture
        .reports
        .recv_timeout(Duration::from_secs(25))
        .unwrap();
    for part in ["main", "frame"] {
        assert_eq!(report[part]["marker"], true, "{part}: {report}");
        assert_eq!(report[part]["timezone"], "Asia/Tokyo");
    }
    // The existing signed companion targets documents, not Worker globals.
    // Session resume must preserve that baseline, and the native Worker region.
    assert_eq!(report["worker"]["marker"], first["worker"]["marker"]);
    assert_eq!(report["worker"]["timezone"], "Asia/Tokyo");
    assert_eq!(report["main"]["webdriver"], false);
    assert_eq!(report["main"]["draft"], "saved draft");
    assert_eq!(report["main"]["scroll"], 700);
    assert!(report["main"]["cookie"]
        .as_str()
        .unwrap()
        .contains("nodecloak-session=restored"));
    platform::close_profile(&path).unwrap();
}
