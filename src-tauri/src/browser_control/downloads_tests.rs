use super::*;
use std::net::TcpListener;
use std::thread::JoinHandle;

#[derive(Default)]
struct WireState {
    requests: Vec<Value>,
    tabs: Vec<Value>,
    tab_trees: VecDeque<Vec<Value>>,
    events: Vec<Value>,
    fail_tree: bool,
}
struct Fixture {
    control: BrowserControl,
    wire: Arc<Mutex<WireState>>,
    server: Option<JoinHandle<()>>,
}
impl Fixture {
    fn new(firefox: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let wire = Arc::new(Mutex::new(WireState::default()));
        let shared = wire.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            loop {
                let command: Value = match ws.read() {
                    Ok(Message::Text(raw)) => serde_json::from_str(&raw).unwrap(),
                    Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => break,
                    other => panic!("unexpected test websocket frame: {other:?}"),
                };
                let (events, reply) = {
                    let mut state = shared.lock().unwrap();
                    state.requests.push(command.clone());
                    let method = command["method"].as_str().unwrap();
                    let tree = if firefox {
                        "browsingContext.getTree"
                    } else {
                        "Target.getTargets"
                    };
                    if method == tree {
                        if let Some(tabs) = state.tab_trees.pop_front() {
                            state.tabs = tabs;
                        }
                    }
                    let reply = if method == tree && state.fail_tree {
                        if firefox {
                            json!({"id":command["id"],"type":"error","error":"unknown error","message":"tree unavailable"})
                        } else {
                            json!({"id":command["id"],"error":{"code":-32000,"message":"tree unavailable"}})
                        }
                    } else {
                        let result = match method {
                            "browser.setDownloadBehavior"
                            | "Browser.setDownloadBehavior"
                            | "session.subscribe" => json!({}),
                            "browsingContext.getTree" if firefox => {
                                json!({"contexts":state.tabs.iter().map(|t| json!({"context":t["id"],"originalOpener":t["opener"],"url":"https://example.test/","children":[]})).collect::<Vec<_>>()})
                            }
                            "Target.getTargets" if !firefox => {
                                json!({"targetInfos":state.tabs.iter().map(|t| json!({"targetId":t["id"],"openerId":t["opener"],"url":"https://example.test/","title":"fixture","type":"page"})).collect::<Vec<_>>()})
                            }
                            _ => panic!("unexpected browser command: {method}"),
                        };
                        if firefox {
                            json!({"id":command["id"],"type":"success","result":result})
                        } else {
                            json!({"id":command["id"],"result":result})
                        }
                    };
                    (std::mem::take(&mut state.events), reply)
                };
                for event in events {
                    ws.send(Message::Text(event.to_string().into())).unwrap();
                }
                ws.send(Message::Text(reply.to_string().into())).unwrap();
            }
        });
        let (mut socket, _) = tungstenite::connect(format!("ws://{address}")).unwrap();
        if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(Duration::from_millis(30)))
                .unwrap();
        }
        let control = BrowserControl {
            diagnostic:None,plan: ControlPlan { profile_settings: false, automation: true },
            protocol: Protocol {
                worker_setups:HashMap::new(),document_setups:HashMap::new(),barrier_owner:None,socket,
                next: 0,
                events: VecDeque::new(),
                last_error: None,
                navigation: None, reconnect_endpoint: None,
            },
            firefox,
            region: regional::initial(&crate::profiles::Preferences::default()).unwrap(),
            permission: PermissionMode::Ask,
            primary: "owner".into(),
            sessions: HashSet::new(),
            targets: HashMap::new(),
            workers: HashSet::new(),
            pages: HashSet::new(),
            advanced: Default::default(),
            fingerprint: None,
            fingerprint_sessions: HashSet::new(),
            internal_workers: HashSet::new(),
            session_checked: Instant::now(),session_capture_ready:false,restore_queue:VecDeque::new(), normal_context:None,            automation_fatal: None,
            automation_dialogs: HashMap::new(),
            automation_warnings: vec![],
            automation_cleanup: vec![],
            download_lease: None,
            download_subscribed: false,
            download_checked: Instant::now(),
        };
        Self {
            control,
            wire,
            server: Some(server),
        }
    }
    fn grant(&self, directory: &Path) -> Grant {
        Grant {
            id: "client-a".into(),
            name: "Download fixture".into(),
            profile_ids: vec!["profile".into()],
            permissions: vec!["read".into(), "page_write".into(), "files".into()],
            origins: vec![],
            expires_at: chrono::Utc::now().timestamp() + 3600,
            confirm_interactions: false,
            file_roots: vec![directory.to_string_lossy().into()],
            revoked: false,
            token_hash: String::new(),
        }
    }
    fn clear_requests(&self) {
        self.wire.lock().unwrap().requests.clear();
    }
    fn methods(&self) -> Vec<String> {
        self.wire
            .lock()
            .unwrap()
            .requests
            .iter()
            .map(|r| r["method"].as_str().unwrap().to_owned())
            .collect()
    }
    fn policies(&self) -> Vec<Value> {
        self.wire
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter_map(|r| match r["method"].as_str() {
                Some("browser.setDownloadBehavior") => {
                    Some(r["params"]["downloadBehavior"].clone())
                }
                Some("Browser.setDownloadBehavior") => Some(r["params"]["behavior"].clone()),
                _ => None,
            })
            .collect()
    }
    fn event(&self, begin: bool) {
        let event = if self.control.firefox {
            json!({"method":if begin {"browsingContext.downloadWillBegin"} else {"browsingContext.downloadEnd"},"params":{"navigation":"transfer","context":"popup","status":"complete"}})
        } else {
            json!({"method":if begin {"Browser.downloadWillBegin"} else {"Browser.downloadProgress"},"params":{"guid":"transfer","state":if begin {"inProgress"} else {"completed"}}})
        };
        self.wire.lock().unwrap().events.push(event);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.control.protocol.socket.close(None);
        if let Some(server) = self.server.take() {
            let result = server.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

#[test]
fn expired_download_lease_denies_before_a_javascript_dialog_can_suspend_maintenance() {
    for firefox in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(firefox);
        let grant = f.grant(root.path());
        f.control
            .claim_downloads(&grant, "profile", "owner", &AtomicBool::new(false))
            .unwrap();
        f.clear_requests();
        f.control.download_lease.as_mut().unwrap().expires = chrono::Utc::now().timestamp() - 1;
        f.control
            .automation_dialogs
            .insert("owner".into(), json!({"type":"alert"}));
        f.control
            .maintain_downloads(&AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            f.methods(),
            vec![if firefox {
                "browser.setDownloadBehavior"
            } else {
                "Browser.setDownloadBehavior"
            }]
        );
        assert_eq!(
            f.policies(),
            vec![if firefox {
                json!({"type":"denied"})
            } else {
                json!("deny")
            }]
        );
        f.clear_requests();
        f.control
            .maintain_downloads(&AtomicBool::new(false))
            .unwrap();
        assert!(
            f.methods().is_empty(),
            "acknowledged denial must not repeat each maintenance tick"
        );
    }
}

#[test]
fn expired_download_lease_denies_even_when_tab_enumeration_fails() {
    for firefox in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(firefox);
        let grant = f.grant(root.path());
        f.control
            .claim_downloads(&grant, "profile", "owner", &AtomicBool::new(false))
            .unwrap();
        f.clear_requests();
        f.control.download_lease.as_mut().unwrap().expires = chrono::Utc::now().timestamp() - 1;
        f.wire.lock().unwrap().fail_tree = true;
        assert!(f
            .control
            .maintain_downloads(&AtomicBool::new(false))
            .unwrap_err()
            .contains("tree unavailable"));
        assert_eq!(
            f.methods(),
            if firefox {
                vec!["browser.setDownloadBehavior", "browsingContext.getTree"]
            } else {
                vec!["Browser.setDownloadBehavior", "Target.getTargets"]
            }
        );
        assert_eq!(
            f.policies(),
            vec![if firefox {
                json!({"type":"denied"})
            } else {
                json!("deny")
            }]
        );
    }
}

#[test]
fn revocation_denies_the_owning_client_immediately_without_reassigning_its_directory() {
    for firefox in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(firefox);
        let grant = f.grant(root.path());
        f.control
            .claim_downloads(&grant, "profile", "owner", &AtomicBool::new(false))
            .unwrap();
        f.clear_requests();
        f.control
            .automation_dialogs
            .insert("owner".into(), json!({"type":"alert"}));
        f.control.invalidate_downloads(Some("client-b")).unwrap();
        assert!(f.methods().is_empty());
        f.control.invalidate_downloads(Some("client-a")).unwrap();
        assert_eq!(
            f.policies(),
            vec![if firefox {
                json!({"type":"denied"})
            } else {
                json!("deny")
            }]
        );
        f.clear_requests();
        let mut other = grant;
        other.id = "client-b".into();
        assert!(f
            .control
            .claim_downloads(&other, "profile", "other", &AtomicBool::new(false))
            .unwrap_err()
            .starts_with("DOWNLOAD_CONTEXT_IN_USE"));
        assert!(
            f.methods().is_empty(),
            "a revoked lease must not redirect delayed downloads to a different client"
        );
    }
}

#[test]
fn download_policy_returns_to_default_only_after_popup_and_transfer_both_finish() {
    for firefox in [false, true] {
        for close_popup_first in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut f = Fixture::new(firefox);
            let grant = f.grant(root.path());
            f.control
                .claim_downloads(&grant, "profile", "owner", &AtomicBool::new(false))
                .unwrap();
            f.clear_requests();
            // The opener closed before enumeration; its child must still own the lease.
            f.wire.lock().unwrap().tabs = vec![
                json!({"id":"popup","opener":"owner"}),
                json!({"id":"manual"}),
            ];
            f.event(true);
            f.control
                .maintain_downloads(&AtomicBool::new(false))
                .unwrap();
            assert!(f.policies().is_empty());
            if close_popup_first {
                f.wire.lock().unwrap().tabs = vec![json!({"id":"manual"})];
            } else {
                f.event(false);
            }
            f.control
                .maintain_downloads(&AtomicBool::new(false))
                .unwrap();
            assert!(
                f.policies().is_empty(),
                "neither closing only the popup nor finishing only the download restores defaults"
            );
            if close_popup_first {
                f.event(false);
            } else {
                f.wire.lock().unwrap().tabs = vec![json!({"id":"manual"})];
            }
            f.control
                .maintain_downloads(&AtomicBool::new(false))
                .unwrap();
            assert_eq!(
                f.policies(),
                vec![if firefox {
                    Value::Null
                } else {
                    json!("default")
                }]
            );
            f.clear_requests();
            f.control
                .maintain_downloads(&AtomicBool::new(false))
                .unwrap();
            assert!(
                f.methods().is_empty(),
                "finished lease must be released while unrelated user tabs remain open"
            );
        }
    }
}

#[test]
fn closing_tab_waits_for_target_retirement_before_restoring_manual_downloads() {
    for firefox in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut f = Fixture::new(firefox);
        let grant = f.grant(root.path());
        f.control
            .claim_downloads(&grant, "profile", "owner", &AtomicBool::new(false))
            .unwrap();
        f.clear_requests();
        // Browser close acknowledgement can precede removal from the target tree.
        f.wire.lock().unwrap().tab_trees = VecDeque::from([
            vec![json!({"id":"owner"}), json!({"id":"manual"})],
            vec![json!({"id":"manual"})],
        ]);
        f.control
            .finish_closed_download_tab("owner", &AtomicBool::new(false))
            .unwrap();
        assert_eq!(
            f.methods(),
            if firefox {
                vec![
                    "browsingContext.getTree",
                    "browsingContext.getTree",
                    "browser.setDownloadBehavior",
                ]
            } else {
                vec![
                    "Target.getTargets",
                    "Target.getTargets",
                    "Browser.setDownloadBehavior",
                ]
            }
        );
        assert_eq!(
            f.policies(),
            vec![if firefox {
                Value::Null
            } else {
                json!("default")
            }]
        );
    }
}
