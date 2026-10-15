use super::*;
use crate::profiles::Preferences;
use std::net::TcpListener;

// Record the actual initialization protocol, without launching a user's browser.
fn initialization_commands(
    browser: &str,
    preferences: Preferences,
    purpose: LaunchPurpose,
) -> Vec<Value> {
    initialization_commands_with_deferral(browser, preferences, purpose, false)
}
fn initialization_commands_with_deferral(
    browser: &str,
    preferences: Preferences,
    purpose: LaunchPurpose,
    defer_page_enable: bool,
) -> Vec<Value> {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let firefox = browser == "firefox";
    if firefox {
        std::fs::write(
            root.path().join("WebDriverBiDiServer.json"),
            json!({"ws_host":"127.0.0.1","ws_port":port}).to_string(),
        )
        .unwrap();
    } else {
        std::fs::write(
            root.path().join("DevToolsActivePort"),
            format!("{port}\n/devtools/browser/test\n"),
        )
        .unwrap();
    }
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut commands = vec![];
        let mut pending_enable = None;
        while let Ok(Message::Text(text)) = socket.read() {
            let command: Value = serde_json::from_str(&text).unwrap();
            if defer_page_enable && command["method"] == "Page.enable" {
                pending_enable = Some(command["id"].clone());
                commands.push(command);
                continue;
            }
            let result = match command["method"].as_str().unwrap() {
                "session.new" => json!({"sessionId":"test"}),
                "browsingContext.getTree" => json!({"contexts":[{"context":"page"}]}),
                // An about:blank popup can inherit the opener's Intl values;
                // those values do not mean this target owns persistent overrides.
                "Runtime.evaluate" => json!({"result":{"value":{"locale":"en-US","timeZone":"Asia/Tokyo"}}}),
                "Target.setAutoAttach" if command.get("sessionId").is_none() => {
                    socket.send(Message::Text(json!({"method":"Target.attachedToTarget","params":{
                        "sessionId":"page","targetInfo":{"targetId":"page","type":"page","url":"about:blank"}
                    }}).to_string().into())).unwrap();
                    json!({})
                }
                _ => json!({}),
            };
            socket
                .send(Message::Text(
                    json!({"id":command["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            if command["method"] == "Runtime.runIfWaitingForDebugger" {
                if let Some(id) = pending_enable.take() {
                    socket.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())).unwrap();
                }
            }
            commands.push(command);
        }
        commands
    });
    let plan = ControlPlan::new(browser, &preferences, purpose);
    let mut control = BrowserControl::initialize(
        browser,
        root.path(),
        regional::initial(&preferences).unwrap(),
        PermissionMode::Ask,
        false,
        "",
        preferences.advanced,
        preferences.fingerprint,
        false,
        plan,
    )
    .unwrap();
    control.configure().unwrap();
    drop(control);
    server.join().unwrap()
}

#[test]
fn profile_settings_do_not_subscribe_to_mcp_dialogs() {
    for browser in ["chrome", "edge", "firefox"] {
        let mut preferences = Preferences::default();
        preferences.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        let commands = initialization_commands(browser, preferences, LaunchPurpose::Manual);
        assert!(
            !commands.iter().any(|c| c["method"] == "session.subscribe"
                && c["params"]["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e.as_str().unwrap().contains("userPrompt"))),
            "{browser}: profile-only startup subscribed to MCP dialogs"
        );
    }
}

#[test]
fn automation_alone_does_not_apply_fingerprint_or_region_overrides() {
    for browser in ["chrome", "edge", "firefox"] {
        let commands = initialization_commands(browser, Preferences::default(), LaunchPurpose::Mcp);
        for command in &commands {
            let method = command["method"].as_str().unwrap();
            assert!(
                !method.starts_with("Emulation.")
                    && !method.starts_with("emulation.")
                    && ![
                        "Page.addScriptToEvaluateOnNewDocument",
                        "script.addPreloadScript",
                        "Browser.setPermission",
                        "Runtime.evaluate"
                    ]
                    .contains(&method),
                "{browser}: unexpected settings command {method}"
            );
            if method == "Target.setAutoAttach" {
                assert_eq!(command["params"]["waitForDebuggerOnStart"], false);
                assert!(!command["params"]["filter"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f["type"]
                        .as_str()
                        .is_some_and(|kind| kind.contains("worker"))));
            }
        }
        if browser == "firefox" {
            assert!(commands.iter().any(|c| c["method"] == "session.subscribe"));
        }
    }
}

#[test]
fn automation_preserves_explicit_profile_settings() {
    for browser in ["chrome", "edge", "firefox"] {
        let mut preferences = Preferences::default();
        preferences.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        preferences.regional.timezone_mode = "custom".into();
        preferences.regional.timezone = "Asia/Tokyo".into();
        let commands = initialization_commands(browser, preferences, LaunchPurpose::Mcp);
        assert!(
            commands.iter().any(|c| c["method"]
                == if browser == "firefox" {
                    "script.addPreloadScript"
                } else {
                    "Page.addScriptToEvaluateOnNewDocument"
                }),
            "{browser}: lost fingerprint"
        );
        assert!(
            commands.iter().any(|c| c["method"]
                == if browser == "firefox" {
                    "emulation.setTimezoneOverride"
                } else {
                    "Emulation.setTimezoneOverride"
                }),
            "{browser}: lost timezone"
        );
    }
}

#[test]
fn profile_only_controller_rejects_automation_before_dispatch() {
    let runtime = Runtime::default();
    let (sender, receiver) = mpsc::channel();
    runtime
        .insert(
            "profile".into(),
            Control {
                automation: false,
                sender,
                stop: Arc::new(AtomicBool::new(false)),
                alive: Arc::new(AtomicBool::new(true)),
                error: Arc::new(Mutex::new(None)),
                worker: None,
            },
        )
        .unwrap();
    assert!(runtime.ready("profile"));
    assert!(!runtime.automation_ready("profile"));
    let grant = crate::mcp::policy::Grant {
        id: "test".into(),
        name: "Test".into(),
        profile_ids: vec!["profile".into()],
        permissions: vec!["page_read".into()],
        origins: vec![],
        expires_at: i64::MAX,
        confirm_interactions: false,
        file_roots: vec![],
        revoked: false,
        token_hash: String::new(),
    };
    let result = runtime.automation(
        "profile",
        "tabs_list",
        json!({"profile_id":"profile"}),
        grant,
        Arc::new(AtomicBool::new(false)),
    );
    assert!(result.unwrap_err().starts_with("RESTART_REQUIRED"));
    runtime.invalidate_downloads(None);
    assert!(
        receiver.try_recv().is_err(),
        "MCP command reached profile-only controller"
    );
}

#[test]
fn paused_profile_targets_resume_before_waiting_for_page_enable_reply() {
    for engine in ["chrome", "edge"] {
        let mut preferences = Preferences::default();
        preferences.fingerprint = Some(crate::fingerprint::fresh().unwrap());
        preferences.regional.timezone_mode = "custom".into();
        preferences.regional.timezone = "Asia/Tokyo".into();
        let commands = initialization_commands_with_deferral(engine, preferences, LaunchPurpose::Manual, true);
        assert!(!commands.iter().any(|c| c["method"] == "Runtime.evaluate"),
            "{engine}: paused target initialization executes JS before resume");
        let enable = commands.iter().position(|c| c["method"] == "Page.enable").expect("Page agent must enable preloads");
        let preload = commands.iter().position(|c| c["method"] == "Page.addScriptToEvaluateOnNewDocument").unwrap();
        let resume = commands.iter().position(|c| c["method"] == "Runtime.runIfWaitingForDebugger").unwrap();
        assert!(enable < preload && preload < resume, "{engine}: preload setup must precede resume");
        assert!(commands.iter().any(|c| c["method"] == "Emulation.setTimezoneOverride"),
            "{engine}: inherited timezone was mistaken for a persistent target override");
    }
}

#[test]
fn rejected_popup_protection_never_resumes_first_document() {
    for rejected in ["Page.addScriptToEvaluateOnNewDocument", "Emulation.setTimezoneOverride"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let mut methods = vec![];
            let mut enable = None;
            while let Ok(Message::Text(text)) = socket.read() {
                let command: Value = serde_json::from_str(&text).unwrap();
                let method = command["method"].as_str().unwrap().to_owned();
                methods.push(method.clone());
                if method == "Page.enable" { enable = Some(command["id"].clone()); continue; }
                let reply = if method == rejected { json!({"id":command["id"],"error":{"code":-32000,"message":"synthetic protection rejection"}}) }
                    else { json!({"id":command["id"],"result":{}}) };
                if socket.send(Message::Text(reply.to_string().into())).is_err() { break; }
                if method == "Runtime.runIfWaitingForDebugger" {
                    if let Some(id) = enable.take() { let _ = socket.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())); }
                }
            }
            methods
        });
        let socket = recovery::connect_socket(&format!("ws://{address}")).unwrap();
        let mut protocol = Protocol { document_setups:HashMap::new(),barrier_owner:None,socket, next: 0, events: VecDeque::new(), last_error: None, navigation: None, reconnect_endpoint: None };
        let result = protocol.configure_document("popup", vec![
            ("Page.enable",json!({})),
            ("Page.addScriptToEvaluateOnNewDocument",json!({"source":"/* fixture */"})),
            ("Emulation.setTimezoneOverride",json!({"timezoneId":"Europe/London"})),
            ("Runtime.runIfWaitingForDebugger",json!({})),
        ]);
        assert!(result.is_ok());
        for _ in 0..20 {
            let _ = protocol.read();
            if protocol.document_setups["popup"].error.is_some() { break; }
        }
        let failed = protocol.document_setups["popup"].error.is_some();
        drop(protocol);
        let commands = server.join().unwrap();
        assert!(failed, "{rejected}: rejection ignored");
        assert!(!commands.iter().any(|m| m == "Runtime.runIfWaitingForDebugger"), "{rejected}: protection failure resumed the popup");
    }
}

#[test]
fn delayed_popup_keeps_its_pause_without_blocking_other_tabs_then_initializes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut held = vec![];
        let mut enable = None;
        let mut activated = false;
        let mut guarded = false;
        let mut methods = vec![];
        while let Ok(Message::Text(text)) = socket.read() {
            let command: Value = serde_json::from_str(&text).unwrap();
            let method = command["method"].as_str().unwrap().to_owned();
            methods.push(method.clone());
            if method == "Page.enable" { enable = Some(command["id"].clone()); continue; }
            if !activated && (method.starts_with("Emulation.") || method == "Page.addScriptToEvaluateOnNewDocument") {
                held.push(command["id"].clone()); continue;
            }
            if method == "Target.activateTarget" {
                activated = true;
                for id in held.drain(..).rev() {
                    socket.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())).unwrap();
                }
            }
            if method == "Debugger.setInstrumentationBreakpoint" { guarded = true; }
            if method == "Debugger.resume" { assert!(activated, "first script resumed before protection acknowledged"); }
            if method == "Runtime.runIfWaitingForDebugger" {
                assert!(activated || guarded, "popup resumed without first-script protection");
                if let Some(id) = enable.take() {
                    socket.send(Message::Text(json!({"id":id,"result":{}}).to_string().into())).unwrap();
                }
            }
            let result = if method == "Debugger.setInstrumentationBreakpoint" { json!({"breakpointId":"first-script"}) } else { json!({}) };
            socket.send(Message::Text(json!({"id":command["id"],"result":result}).to_string().into())).unwrap();
        }
        methods
    });
    let socket = recovery::connect_socket(&format!("ws://{address}")).unwrap();
    let mut protocol = Protocol { document_setups:HashMap::new(),barrier_owner:None,socket, next: 0, events: VecDeque::new(), last_error: None, navigation: None, reconnect_endpoint: None };
    let started = Instant::now();
    protocol.configure_document("popup", vec![
        ("Page.enable",json!({})),
        ("Page.addScriptToEvaluateOnNewDocument",json!({"source":"/* fixture */"})),
        ("Emulation.setLocaleOverride",json!({"locale":"en-US"})),
        ("Emulation.setTimezoneOverride",json!({"timezoneId":"Europe/London"})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "popup blocked event processing");
    protocol.document_setups.get_mut("popup").unwrap().started = Instant::now() - Duration::from_secs(20);
    protocol.check_document_delays();
    assert!(protocol.document_pending("popup"));
    assert_eq!(protocol.events.back().unwrap()["method"], "NodeCloak.documentSetupDelayed");
    protocol.call("Target.getTargets", json!({}), None).expect("another tab must remain usable");
    assert!(protocol.document_pending("popup"));
    protocol.configure_document("popup", vec![
        ("Emulation.setGeolocationOverride",json!({"latitude":1.3521,"longitude":103.8198,"accuracy":100})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    protocol.call("Target.activateTarget", json!({"targetId":"popup"}), None).unwrap();
    while protocol.document_pending("popup") { protocol.read().unwrap(); }
    assert_eq!(protocol.events.back().unwrap()["method"], "NodeCloak.documentSetupReady");
    drop(protocol);
    let methods = server.join().unwrap();
    assert!(methods.iter().any(|m| m == "Emulation.setGeolocationOverride"), "final resolved settings were discarded while provisional setup was pending");
    assert!(!methods.iter().any(|m| m == "Target.closeTarget" || m == "Target.detachFromTarget"));
}



#[test]
fn settings_update_during_barrier_release_waits_for_a_new_generation() {
    for update_while_arming in [false, true] {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut releasing = false;
        let mut disabled = false;
        let mut premature_update = false;
        while let Ok(Message::Text(text)) = socket.read() {
            let command: Value = serde_json::from_str(&text).unwrap();
            let method = command["method"].as_str().unwrap();
            if method == "Runtime.runIfWaitingForDebugger" { releasing = true; }
            if method == "Debugger.disable" { disabled = true; }
            if method == "Emulation.setTimezoneOverride" && releasing && !disabled {
                premature_update = true;
                // Main-thread commands cannot acknowledge during the pause.
                continue;
            }
            let result = if method == "Debugger.setInstrumentationBreakpoint" {
                json!({"breakpointId":"first-script"})
            } else { json!({}) };
            if socket.send(Message::Text(json!({"id":command["id"],"result":result}).to_string().into())).is_err() { break; }
        }
        premature_update
    });
    let socket = recovery::connect_socket(&format!("ws://{address}")).unwrap();
    let mut protocol = Protocol { document_setups:HashMap::new(),barrier_owner:None,socket, next: 0, events: VecDeque::new(), last_error: None, navigation: None, reconnect_endpoint: None };
    protocol.configure_document("popup", vec![
        ("Emulation.setTimezoneOverride",json!({"timezoneId":"Europe/London"})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    protocol.document_setups.get_mut("popup").unwrap().started = Instant::now() - Duration::from_secs(1);
    protocol.advance_document("popup").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    if !update_while_arming {
        while protocol.document_setups["popup"].barrier != StartupBarrier::Removing {
            assert!(Instant::now() < deadline);
            protocol.read().unwrap();
        }
    }
    protocol.configure_document("popup", vec![
        ("Emulation.setTimezoneOverride",json!({"timezoneId":"Asia/Tokyo"})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    assert!(protocol.document_pending("popup"));
    let deadline = Instant::now() + Duration::from_secs(2);
    while protocol.document_pending("popup") && Instant::now() < deadline { protocol.read().unwrap(); }
    let ready = !protocol.document_pending("popup");
    drop(protocol);
    let premature = server.join().unwrap();
    assert!(!premature, "final settings were queued while first-script barrier was releasing");
    assert!(ready, "final settings deadlocked barrier cleanup");
    }
}

#[test]
fn late_protection_rejection_retains_the_only_first_script_barrier() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut held = vec![];
        let mut methods = vec![];
        while let Ok(Message::Text(text)) = socket.read() {
            let command: Value = serde_json::from_str(&text).unwrap();
            let method = command["method"].as_str().unwrap().to_owned();
            methods.push((command["sessionId"].clone(),method.clone()));
            if method == "Page.addScriptToEvaluateOnNewDocument" { held.push(command); continue; }
            if method == "Runtime.runIfWaitingForDebugger" {
                for command in held.drain(..) {
                    socket.send(Message::Text(json!({"id":command["id"],"error":{"code":-32602,"message":"fixture preload rejection"}}).to_string().into())).unwrap();
                }
            }
            let result = if method == "Debugger.setInstrumentationBreakpoint" { json!({"breakpointId":"first-script"}) } else { json!({}) };
            if socket.send(Message::Text(json!({"id":command["id"],"result":result}).to_string().into())).is_err() { break; }
        }
        methods
    });
    let socket = recovery::connect_socket(&format!("ws://{address}")).unwrap();
    let mut protocol = Protocol { document_setups:HashMap::new(),barrier_owner:None,socket, next: 0, events: VecDeque::new(), last_error: None, navigation: None, reconnect_endpoint: None };
    protocol.configure_document("failed",vec![
        ("Page.addScriptToEvaluateOnNewDocument",json!({"source":1})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    protocol.document_setups.get_mut("failed").unwrap().started = Instant::now()-Duration::from_secs(1);
    protocol.advance_document("failed").unwrap();
    for _ in 0..10 {
        protocol.read().unwrap();
        if protocol.document_setups["failed"].error.is_some() { break; }
    }
    assert!(protocol.document_setups["failed"].error.is_some());
    assert_eq!(protocol.barrier_owner.as_deref(),Some("failed"));
    protocol.configure_document("next",vec![
        ("Page.addScriptToEvaluateOnNewDocument",json!({"source":"/* next */"})),
        ("Runtime.runIfWaitingForDebugger",json!({})),
    ]).unwrap();
    protocol.document_setups.get_mut("next").unwrap().started = Instant::now()-Duration::from_secs(1);
    protocol.advance_document("next").unwrap();
    protocol.call("Target.getTargets",json!({}),None).unwrap();
    drop(protocol);
    let methods = server.join().unwrap();
    assert!(!methods.iter().any(|(_,m)|["Debugger.resume","Debugger.removeBreakpoint","Debugger.disable","Target.closeTarget","Target.detachFromTarget"].contains(&m.as_str())));
    assert!(!methods.iter().any(|(s,m)|s=="next" && m=="Runtime.runIfWaitingForDebugger"), "a second page could release the failed page's renderer");
}
