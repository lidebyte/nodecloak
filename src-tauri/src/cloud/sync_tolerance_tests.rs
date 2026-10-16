//! Real encrypted documents, synthetic HTTP responses, isolated OS credentials.
use super::{
    account::{Connection, Session},
    client::Client,
    crypto,
    model::{Account, Binding, Document, Object},
    sync, vault,
};
use crate::profiles;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
struct Fixture {
    root: tempfile::TempDir,
    c: Connection,
    p: profiles::Profile,
    key: crypto::Key,
    queue: Arc<Mutex<VecDeque<(String, u16, Value)>>>,
    requests: Arc<Mutex<Vec<(String, Value)>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut c = Connection::test_connection(
            root.path(),
            Client::new(&format!("http://{}", listener.local_addr().unwrap()), true).unwrap(),
            &format!("test-tolerance-{}", uuid::Uuid::new_v4()),
        )
        .unwrap();
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            email: "test@example.invalid".into(),
            name: "Test".into(),
            avatar: String::new(),
            google_linked: false,
            has_password: false,
            two_factor_enabled: false,
            recent_auth: false,
        };
        c.disk.account = Some(account.clone());
        c.save_session(Session {
            account_id: account.id.clone(),
            access_token: "synthetic-access".into(),
            refresh_token: "synthetic-refresh".into(),
            expires_at: chrono::Utc::now().timestamp() + 900,
        })
        .unwrap();
        let key = crypto::random_key().unwrap();
        vault::save_root(&c.ns, &account.id, 1, &key).unwrap();
        let p=profiles::create(root.path(),serde_json::from_value(json!({"name":"Base","browser":"firefox","notes":"Base note","proxy":{"mode":"direct","host":"","port":0,"username":""}})).unwrap(),None).unwrap();
        let s = c.disk.accounts.entry(account.id).or_default();
        s.setup_complete = true;
        s.auto_sync = true;
        s.recovery_saved = true;
        s.epoch = 1;
        s.authorized_device = Some(c.device.id.clone());
        s.known_local.extend(
            profiles::load(root.path())
                .unwrap()
                .into_iter()
                .map(|p| p.id),
        );
        s.bindings.insert(
            p.id.clone(),
            Binding::new(&uuid::Uuid::new_v4().to_string()),
        );
        c.disk.save(root.path()).unwrap();
        let queue = Arc::new(Mutex::new(VecDeque::<(String, u16, Value)>::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (q, seen, quit) = (queue.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            while !quit.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let header = String::from_utf8(header).unwrap();
                let route = header
                    .lines()
                    .next()
                    .unwrap()
                    .split(" HTTP/")
                    .next()
                    .unwrap()
                    .to_owned();
                let len = header
                    .lines()
                    .find_map(|s| {
                        s.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|s| s.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; len];
                stream.read_exact(&mut body).unwrap();
                seen.lock().unwrap().push((
                    route.clone(),
                    if len == 0 {
                        Value::Null
                    } else {
                        serde_json::from_slice(&body).unwrap()
                    },
                ));
                let (expected, status, body) = q
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| panic!("unexpected synthetic request {route}"));
                assert_eq!(route, expected);
                let body = body.to_string();
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        Self {
            root,
            c,
            p,
            key,
            queue,
            requests,
            stop,
            worker: Some(worker),
        }
    }
    fn account(&self) -> String {
        self.c.account_id().unwrap()
    }
    fn cloud(&self) -> String {
        self.c.disk.accounts[&self.account()].bindings[&self.p.id]
            .cloud_id
            .clone()
    }
    fn push(&self, method: &str, path: &str, status: u16, body: Value) {
        self.queue
            .lock()
            .unwrap()
            .push_back((format!("{method} {path}"), status, body));
    }
    fn object(&self, doc: &Document, revision: u64) -> Object {
        let mut o = Object {
            id: self.cloud(),
            kind: "config".into(),
            revision,
            epoch: 1,
            deleted: doc.deleted,
            payload: String::new(),
            writer: self.c.device.id.clone(),
            generation: 0,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        o.payload = serde_json::to_string(
            &crypto::seal(
                &self.key,
                &o.context(&self.account()),
                &serde_json::to_vec(doc).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        o
    }
    fn baseline(&mut self, o: &Object) {
        let a = self.account();
        self.c
            .disk
            .accounts
            .get_mut(&a)
            .unwrap()
            .bindings
            .get_mut(&self.p.id)
            .unwrap()
            .baseline = Some(o.clone());
        self.c.disk.save(self.root.path()).unwrap();
    }
    fn preflight(&self, objects: Vec<Object>) {
        self.push("GET", "/v1/account", 200, json!(self.c.disk.account));
        self.push("GET","/v1/vault",200,json!({"epoch":1,"approved":true,"deviceEnvelope":crypto::device_seal(&*vault::decode_key(&self.c.device.public_key).unwrap(),&self.account(),&self.c.device.id,1,&self.key).unwrap(),"deviceProof":crypto::device_proof(&self.key,&self.account(),&self.c.device.id,&self.c.device.public_key,1)}));
        self.push("GET", "/v1/objects", 200, json!({"objects":objects}));
    }
    fn complete(&self) {
        assert!(
            self.queue.lock().unwrap().is_empty(),
            "missing expected HTTP steps"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        let _ = vault::remove(&format!("{}:device", self.c.ns));
        let _ = vault::remove(&format!("{}:session", self.c.ns));
        let _ = vault::remove(&vault::root_name(&self.c.ns, &self.account(), 1));
    }
}
#[test]
fn configuration_cas_race_reloads_and_merges_without_manual_resolution() {
    let mut f = Fixture::new();
    let base = Document::export(&f.p).unwrap();
    let initial = f.object(&base, 1);
    f.baseline(&initial);
    let mut local = base.draft();
    local.name = "Local rename".into();
    profiles::update(f.root.path(), &f.p.id, local).unwrap();
    let mut peer = base;
    peer.notes = "Peer note".into();
    let newer = f.object(&peer, 2);
    f.preflight(vec![initial.clone()]);
    let route = format!("/v1/objects/{}", f.cloud());
    f.push(
        "GET",
        &(route.clone() + "?kind=config"),
        200,
        json!(initial),
    );
    f.push(
        "PUT",
        &route,
        409,
        json!({"error":"revision_conflict","revision":2}),
    );
    f.push("GET", &(route.clone() + "?kind=config"), 200, json!(newer));
    f.push("PUT", &route, 200, json!({"revision":3}));
    sync::run(&mut f.c, &profiles::Runtime::default()).unwrap();
    assert!(
        f.c.disk.accounts[&f.account()].conflicts.is_empty(),
        "optimistic race is not a semantic conflict"
    );
    let p = profiles::get(f.root.path(), &f.p.id, false).unwrap();
    assert_eq!(p.name, "Local rename");
    assert_eq!(p.notes, "Peer note");
    assert_eq!(
        f.c.disk.accounts[&f.account()].bindings[&f.p.id]
            .baseline
            .as_ref()
            .unwrap()
            .revision,
        3
    );
    let writes: Vec<_> = f
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(r, _)| r.starts_with("PUT "))
        .map(|(_, v)| v["baseRevision"].as_u64().unwrap())
        .collect();
    assert_eq!(writes, [1, 2]);
    f.complete();
}
#[test]
fn launching_after_peer_configuration_edit_catches_up_without_user_sync() {
    let mut f = Fixture::new();
    let base = Document::export(&f.p).unwrap();
    let initial = f.object(&base, 1);
    f.baseline(&initial);
    let a = f.account();
    f.c.disk.accounts.get_mut(&a).unwrap().session_sync = true;
    let path = profiles::path(f.root.path(), &f.p)
        .unwrap()
        .join("storage/default/https+++example.invalid^userContextId=2");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("sentinel"), b"preserved").unwrap();
    let mut peer = base;
    peer.notes = "Peer changed note".into();
    let newer = f.object(&peer, 2);
    f.preflight(vec![newer.clone()]);
    f.push(
        "GET",
        &format!("/v1/objects/{}?kind=config", f.cloud()),
        200,
        json!(newer),
    );
    let result = super::session_sync::prepare_launch(&mut f.c, &f.p.id);
    assert!(
        result.is_ok(),
        "peer edit must not block launch: {result:?}"
    );
    assert_eq!(
        profiles::get(f.root.path(), &f.p.id, false).unwrap().notes,
        "Peer changed note"
    );
    assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"preserved");
    f.complete();
}
#[test]
fn closed_unchanged_profile_releases_its_exact_session_generation() {
    let mut f = Fixture::new();
    let a = f.account();
    let stamp =
        super::session_sync::stamp("firefox", &profiles::path(f.root.path(), &f.p).unwrap())
            .unwrap();
    let mut b = f.c.disk.accounts[&a].bindings[&f.p.id].clone();
    b.generation = 42;
    b.file_stamp = Some(stamp);
    f.push(
        "DELETE",
        &format!("/v1/objects/{}/lease", f.cloud()),
        200,
        json!({"released":true}),
    );
    super::session_sync::one(&mut f.c, &f.p, &mut b, None).unwrap();
    assert_eq!(
        b.generation, 0,
        "closed device must not retain lease until timeout"
    );
    assert_eq!(f.requests.lock().unwrap()[0].1["generation"], 42);
    f.complete();
}

#[test]
fn transient_conditions_are_waiting_but_real_edits_still_require_a_choice() {
    let mut f = Fixture::new();
    let a = f.account();
    let cloud = f.cloud();
    let fields = [
        "close_browser",
        "concurrent_write",
        "config_changed",
        "lease_held",
        "lease_or_revision_changed",
        "deletion_waiting_device",
        "/name",
        "login_state_changed",
        "deletion_vs_edit",
    ];
    for field in fields {
        f.c.disk
            .accounts
            .get_mut(&a)
            .unwrap()
            .conflicts
            .push(super::model::Conflict {
                local_id: f.p.id.clone(),
                cloud_id: cloud.clone(),
                name: f.p.name.clone(),
                kind: if field == "/name" || field == "deletion_vs_edit" {
                    "config"
                } else {
                    "session"
                }
                .into(),
                fields: vec![field.into()],
                remote_revision: 2,
            });
    }
    let status = f.c.status().unwrap();
    let choices: Vec<_> = status["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["fields"][0].as_str().unwrap())
        .collect();
    assert_eq!(
        choices,
        ["/name", "login_state_changed", "deletion_vs_edit"]
    );
    assert_eq!(status["waiting"].as_array().unwrap().len(), 6);
    f.c.disk.save(f.root.path()).unwrap();
    assert_eq!(
        super::model::Disk::load(f.root.path()).unwrap().accounts[&a]
            .conflicts
            .len(),
        9,
        "persisted records stay compatible with older clients"
    );
}

#[test]
fn repeated_cas_races_are_bounded_and_do_not_apply_an_uncommitted_merge() {
    let mut f = Fixture::new();
    let base = Document::export(&f.p).unwrap();
    f.baseline(&f.object(&base, 1));
    let mut local = base.draft();
    local.name = "Local rename".into();
    profiles::update(f.root.path(), &f.p.id, local).unwrap();
    let mut remotes = Vec::new();
    for revision in 2..=4 {
        let mut peer = Document::export(&f.p).unwrap();
        peer.notes = format!("Peer note {revision}");
        remotes.push(f.object(&peer, revision));
    }
    f.preflight(vec![remotes[0].clone()]);
    let route = format!("/v1/objects/{}", f.cloud());
    for o in remotes {
        f.push("GET", &(route.clone() + "?kind=config"), 200, json!(o));
        f.push("PUT", &route, 409, json!({"error":"revision_conflict"}));
    }
    sync::run(&mut f.c, &profiles::Runtime::default()).unwrap();
    let p = profiles::get(f.root.path(), &f.p.id, false).unwrap();
    assert_eq!(p.notes, "Base note");
    assert_eq!(p.name, "Local rename");
    let status = f.c.status().unwrap();
    assert!(status["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(status["waiting"][0]["fields"][0], "concurrent_write");
    f.complete();
}

#[test]
fn older_server_without_release_keeps_a_retryable_fence_and_successful_sync() {
    let mut f = Fixture::new();
    let a = f.account();
    let mut b = f.c.disk.accounts[&a].bindings[&f.p.id].clone();
    b.generation = 42;
    b.file_stamp = Some(
        super::session_sync::stamp("firefox", &profiles::path(f.root.path(), &f.p).unwrap())
            .unwrap(),
    );
    f.push(
        "DELETE",
        &format!("/v1/objects/{}/lease", f.cloud()),
        405,
        json!({}),
    );
    super::session_sync::one(&mut f.c, &f.p, &mut b, None).unwrap();
    assert_eq!(b.generation, 42);
    assert!(f.c.disk.accounts[&a].conflicts.is_empty());
    f.complete();
}

#[test]
fn config_only_sync_also_catches_up_before_launch_without_dirtying_login_data() {
    let mut f = Fixture::new();
    let mut doc = Document::export(&f.p).unwrap();
    f.baseline(&f.object(&doc, 1));
    doc.notes = "Peer config only".into();
    let remote = f.object(&doc, 2);
    f.preflight(vec![remote.clone()]);
    f.push(
        "GET",
        &format!("/v1/objects/{}?kind=config", f.cloud()),
        200,
        json!(remote),
    );
    super::session_sync::prepare_launch(&mut f.c, &f.p.id).unwrap();
    assert_eq!(
        profiles::get(f.root.path(), &f.p.id, false).unwrap().notes,
        "Peer config only"
    );
    let b = &f.c.disk.accounts[&f.account()].bindings[&f.p.id];
    assert!(!b.session_dirty);
    assert_eq!(b.generation, 0);
    f.complete();
}

#[test]
fn new_browser_launch_requests_a_fresh_fence_for_delayed_close_protection() {
    let mut f = Fixture::new();
    let doc = Document::export(&f.p).unwrap();
    let config = f.object(&doc, 1);
    f.baseline(&config);
    let a = f.account();
    let stamp =
        super::session_sync::stamp("firefox", &profiles::path(f.root.path(), &f.p).unwrap())
            .unwrap();
    let s = f.c.disk.accounts.get_mut(&a).unwrap();
    s.session_sync = true;
    let b = s.bindings.get_mut(&f.p.id).unwrap();
    b.generation = 42;
    b.file_stamp = Some(stamp);
    f.preflight(vec![config]);
    f.push(
        "POST",
        &format!("/v1/objects/{}/lease", f.cloud()),
        200,
        json!({"generation":43}),
    );
    super::session_sync::prepare_launch(&mut f.c, &f.p.id).unwrap();
    assert_eq!(
        f.requests.lock().unwrap().last().unwrap().1["newSession"],
        true
    );
    f.complete();
}

#[test]
fn acknowledged_session_deletion_also_releases_its_remaining_fence() {
    let mut f = Fixture::new();
    let mut doc = Document::export(&f.p).unwrap();
    doc.deleted = true;
    let config = f.object(&doc, 2);
    f.baseline(&config);
    let a = f.account();
    let s = f.c.disk.accounts.get_mut(&a).unwrap();
    s.session_sync = true;
    s.bindings.get_mut(&f.p.id).unwrap().generation = 42;
    let tombstone = Object {
        id: f.cloud(),
        kind: "session".into(),
        revision: 4,
        epoch: 1,
        deleted: true,
        payload: String::new(),
        writer: f.c.device.id.clone(),
        generation: 42,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    f.push("GET", "/v1/objects", 200, json!({"objects":[tombstone]}));
    f.push(
        "DELETE",
        &format!("/v1/objects/{}/lease", f.cloud()),
        200,
        json!({"released":true}),
    );
    super::session_sync::run(&mut f.c).unwrap();
    assert_eq!(f.c.disk.accounts[&a].bindings[&f.p.id].generation, 0);
    f.complete();
}

#[test]
fn launch_remains_compatible_with_servers_without_fresh_session_acquisition() {
    let mut f = Fixture::new();
    let doc = Document::export(&f.p).unwrap();
    let config = f.object(&doc, 1);
    f.baseline(&config);
    let a = f.account();
    let stamp =
        super::session_sync::stamp("firefox", &profiles::path(f.root.path(), &f.p).unwrap())
            .unwrap();
    let s = f.c.disk.accounts.get_mut(&a).unwrap();
    s.session_sync = true;
    s.bindings.get_mut(&f.p.id).unwrap().file_stamp = Some(stamp);
    f.preflight(vec![config]);
    let path = format!("/v1/objects/{}/lease", f.cloud());
    f.push("POST", &path, 400, json!({"error":"invalid_request"}));
    f.push("POST", &path, 200, json!({"generation":42}));
    super::session_sync::prepare_launch(&mut f.c, &f.p.id).unwrap();
    let requests = f.requests.lock().unwrap();
    assert_eq!(requests[requests.len() - 2].1["newSession"], true);
    assert!(requests.last().unwrap().1.get("newSession").is_none());
    assert_eq!(f.c.disk.accounts[&a].bindings[&f.p.id].generation, 42);
    f.complete();
}
