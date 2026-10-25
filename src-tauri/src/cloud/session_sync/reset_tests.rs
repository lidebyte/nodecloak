use super::super::{
    account::Session,
    client::Client,
    crypto,
    model::{Account, AccountState},
    vault,
};
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

struct Credentials {
    ns: String,
    account: String,
}
impl Drop for Credentials {
    fn drop(&mut self) {
        for key in [
            format!("{}:device", self.ns),
            format!("{}:session", self.ns),
            vault::root_name(&self.ns, &self.account, 1),
        ] {
            let _ = vault::remove(&key);
        }
    }
}
fn fixture(
    endpoint: &str,
) -> (
    tempfile::TempDir,
    Connection,
    profiles::Profile,
    Binding,
    Credentials,
    crypto::Key,
) {
    let dir = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let credentials = Credentials {
        ns: format!("reset-{}", uuid::Uuid::new_v4()),
        account: uuid::Uuid::new_v4().to_string(),
    };
    let mut c = Connection::test_connection(
        dir.path(),
        Client::new(endpoint, true).unwrap(),
        &credentials.ns,
    )
    .unwrap();
    let account = credentials.account.clone();
    c.disk.account = Some(Account {
        id: account.clone(),
        email: "reset@example.invalid".into(),
        name: "Reset test".into(),
        avatar: String::new(),
        google_linked: false,
        has_password: false,
        two_factor_enabled: false,
        recent_auth: false,
    });
    c.save_session(Session {
        account_id: account.clone(),
        access_token: "synthetic-token".into(),
        refresh_token: "synthetic-refresh".into(),
        expires_at: chrono::Utc::now().timestamp() + 900,
    })
    .unwrap();
    let p = profiles::create(
        dir.path(),
        serde_json::from_value(
            json!({"name":"Reset","browser":"firefox","proxy":{"mode":"direct"}}),
        )
        .unwrap(),
        None,
    )
    .unwrap();
    let mut binding = Binding::new(&uuid::Uuid::new_v4().to_string());
    binding.baseline = Some(Object {
        id: binding.cloud_id.clone(),
        kind: "config".into(),
        revision: 3,
        epoch: 1,
        deleted: false,
        payload: String::new(),
        writer: c.device.id.clone(),
        generation: 0,
        updated_at: chrono::Utc::now().to_rfc3339(),
    });
    let mut state = AccountState::default();
    state.epoch = 1;
    state.authorized_device = Some(c.device.id.clone());
    state.bindings.insert(p.id.clone(), binding.clone());
    c.disk.accounts.insert(account.clone(), state);
    c.disk.save(dir.path()).unwrap();
    let key = crypto::random_key().unwrap();
    vault::save_root(&credentials.ns, &account, 1, &key).unwrap();
    profiles::reset::reset(dir.path(), &p.id, false, &profiles::Runtime::default()).unwrap();
    (dir, c, p, binding, credentials, key)
}
fn request(listener: &TcpListener) -> (TcpStream, String, serde_json::Value) {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(v) => break v,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            Err(e) => panic!("synthetic reset request missing: {e}"),
        }
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut headers = Vec::new();
    let mut byte = [0];
    while !headers.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16384);
    }
    let headers = String::from_utf8(headers).unwrap();
    let route = headers.lines().next().unwrap().to_owned();
    let size = headers
        .lines()
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(size < 16384);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    (stream, route, serde_json::from_slice(&body).unwrap())
}
fn respond(mut stream: TcpStream, status: u16, body: serde_json::Value) {
    let body = body.to_string();
    write!(stream,"HTTP/1.1 {status} test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
}

#[test]
fn pending_reset_keeps_new_local_data_and_does_not_import_or_capture_a_newer_remote() {
    let (dir, mut c, p, mut binding, credentials, _key) = fixture("http://127.0.0.1:9");
    let account = &credentials.account;
    let file = profiles::path(dir.path(), &p)
        .unwrap()
        .join("storage/default/https+++example.invalid^userContextId=2/keep");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"new-local-login").unwrap();
    let remote = Object {
        id: binding.cloud_id.clone(),
        kind: "session".into(),
        revision: 7,
        epoch: 1,
        deleted: false,
        payload: "must-not-decrypt".into(),
        writer: "other-device".into(),
        generation: 2,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let result = reconcile(&mut c, &p, &mut binding, Some(&remote), true).unwrap();
    assert!(matches!(result, SessionPreparation::LocalOnly));
    assert_eq!(
        binding.pending_session.as_ref().map(|o| o.revision),
        Some(7)
    );
    assert!(c.disk.accounts[account]
        .conflicts
        .iter()
        .any(|x| x.local_id == p.id && x.kind == "session"));
    assert!(
        c.disk.accounts[account].last_error.is_none(),
        "reset protection must run before browser capture"
    );
    assert_eq!(std::fs::read(file).unwrap(), b"new-local-login");
    assert!(profiles::reset::session_pending(dir.path(), &p.id, account).unwrap());
}
#[test]
fn clean_reset_publishes_an_encrypted_empty_session_and_acknowledges_only_after_success() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (dir, mut c, p, mut binding, credentials, key) =
        fixture(&format!("http://{}", listener.local_addr().unwrap()));
    let id = binding.cloud_id.clone();
    let account = credentials.account.clone();
    let writer = c.device.id.clone();
    let secret = *key;
    let server = std::thread::spawn(move || {
        let (stream, route, body) = request(&listener);
        assert!(route.starts_with(&format!("POST /v1/objects/{id}/lease ")));
        assert_eq!(body["takeover"], false);
        respond(stream, 200, json!({"generation":8}));
        let (stream, route, body) = request(&listener);
        assert!(route.starts_with(&format!("PUT /v1/objects/{id} ")));
        assert_eq!(body["baseRevision"], 0);
        assert_eq!(body["configRevision"], 3);
        assert_eq!(body["generation"], 8);
        let envelope = serde_json::from_str(body["payload"].as_str().unwrap()).unwrap();
        let object = Object {
            id,
            kind: "session".into(),
            revision: 1,
            epoch: 1,
            deleted: false,
            payload: String::new(),
            writer,
            generation: 8,
            updated_at: String::new(),
        };
        let plain = crypto::open(&secret, &object.context(&account), &envelope).unwrap();
        let snapshot = Snapshot::parse(serde_json::from_slice(&plain).unwrap()).unwrap();
        assert!(snapshot.cookies.is_empty() && snapshot.origins.is_empty());
        respond(stream, 200, json!({"revision":1}));
    });
    assert!(profiles::reset::session_pending(dir.path(), &p.id, &credentials.account).unwrap());
    let result = reconcile(&mut c, &p, &mut binding, None, false);
    server.join().unwrap();
    result.unwrap();
    assert_eq!(
        binding.session_baseline.as_ref().map(|o| o.revision),
        Some(1)
    );
    assert!(!binding.session_dirty);
    assert!(!profiles::reset::session_pending(dir.path(), &p.id, &credentials.account).unwrap());
    let remote = cached(&c, binding.session_baseline.as_ref().unwrap()).unwrap();
    assert!(matches!(
        reconcile(&mut c, &p, &mut binding, Some(&remote), false).unwrap(),
        SessionPreparation::Complete
    ));
}
#[test]
fn lease_failure_retains_the_reset_barrier_without_restoring_or_publishing_data() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (dir, mut c, p, mut binding, credentials, _key) =
        fixture(&format!("http://{}", listener.local_addr().unwrap()));
    let server = std::thread::spawn(move || {
        let (stream, _, _) = request(&listener);
        respond(stream, 409, json!({"error":"lease_held"}));
    });
    let result = reconcile(&mut c, &p, &mut binding, None, true);
    server.join().unwrap();
    assert!(result.err().unwrap().contains("lease_held"));
    assert!(profiles::reset::session_pending(dir.path(), &p.id, &credentials.account).unwrap());
    assert!(binding.session_baseline.is_none());
}
