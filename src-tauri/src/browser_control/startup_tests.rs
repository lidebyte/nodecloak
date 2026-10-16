//! Startup must use a web-accessible document without altering restored tabs.
use super::*;
use std::net::TcpListener;

#[test]
fn firefox_ip_initialization_preserves_internal_and_restored_tabs() {
    for existing_blank in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::fs::write(root.path().join("WebDriverBiDiServer.json"),
            json!({"ws_host":"127.0.0.1","ws_port":port}).to_string()).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let mut ready = false;
            let mut commands = vec![];
            while let Ok(Message::Text(text)) = socket.read() {
                let command: Value = serde_json::from_str(&text).unwrap();
                let result = match command["method"].as_str().unwrap() {
                    "session.new" => json!({"sessionId":"fixture"}),
                    "browsingContext.getTree" => {
                        let mut contexts = vec![json!({"context":"home","url":"about:home"}),
                            json!({"context":"restored","url":"https://example.test/saved"}),
                            json!({"context":"container","url":"about:blank","userContext":"container-id"})];
                        if existing_blank { contexts.push(json!({"context":"blank","url":"about:blank","userContext":"default"})); }
                        json!({"contexts":contexts})
                    }
                    "browsingContext.create" => {
                        assert_eq!(command["params"]["userContext"], "default");
                        json!({"context":"blank"})
                    }
                    "browsingContext.navigate" => {
                        assert_eq!(command["params"]["context"], "blank");
                        assert_eq!(command["params"]["url"], "about:blank");
                        assert_eq!(command["params"]["wait"], "complete");
                        ready = true;
                        json!({})
                    }
                    "script.evaluate" => {
                        assert!(ready, "evaluated before loading the startup document");
                        assert_eq!(command["params"]["target"]["context"], "blank");
                        json!({"type":"success","result":{"type":"string","value":
                            json!({"success":true,"ip":"203.0.113.12","country_code":"JP","city":"Tokyo",
                                "latitude":35.68,"longitude":139.69,"timezone":{"id":"Asia/Tokyo"}}).to_string()}})
                    }
                    method => panic!("unexpected startup operation {method}"),
                };
                socket.send(Message::Text(json!({"id":command["id"],"result":result}).to_string().into())).unwrap();
                commands.push(command);
            }
            commands
        });
        let preferences = crate::profiles::Preferences::default();
        let control = BrowserControl::initialize("firefox", root.path(),
            regional::initial(&preferences).unwrap(), PermissionMode::Ask, true,
            "https://example.test/region", preferences.advanced.clone(), None, false,
            ControlPlan::new("firefox", &preferences, LaunchPurpose::Manual)).unwrap();
        assert_eq!(control.region.ip_region.as_ref().unwrap().timezone, "Asia/Tokyo");
        drop(control);
        let commands = server.join().unwrap();
        assert_eq!(commands.iter().filter(|command| command["method"] == "browsingContext.create").count(),
            if existing_blank { 0 } else { 1 });
    }
}
