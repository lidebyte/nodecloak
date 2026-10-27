//! Real browser capacity checks use only disposable, network-isolated profiles.
use super::browser_session::{self, Snapshot};
use serde_json::json;

pub(super) fn fixture() -> Snapshot {
    // Escaping makes the browser protocol representation substantially larger
    // than 12 MiB, including Firefox's nested JSON response.
    let value = "\\\"".repeat((9 << 20) / 2);
    Snapshot::parse(json!({"schema":1,"cookies":[],"origins":[{
        "origin":"https://large-session-fixture.invalid",
        "state":{"local":[],"databases":[{"name":"session-fixture","version":1,"stores":[{
            "name":"tokens","keyPath":null,"autoIncrement":false,"indexes":[],
            "rows":[[{"root":"login","nodes":[]},{"root":value,"nodes":[]}]]
        }]}]}
    }]}))
    .unwrap()
}

#[test]
#[ignore = "Starts installed Chrome, Edge and Firefox in disposable profiles with a >8 MiB IndexedDB snapshot"]
fn installed_browsers_large_session_roundtrip() {
    let expected = fixture();
    assert!(expected.bytes().unwrap().len() > 8 << 20);
    for browser in ["chrome", "edge", "firefox"] {
        crate::platform::browser_path(browser)
            .expect("all three browsers are required for this check");
        let root = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let profile = root.path().join("profile");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(profile.join("sentinel"), b"existing-browser-data").unwrap();
        let actual = browser_session::restore_profile(browser, &profile, &expected).expect(browser);
        assert_eq!(
            actual.bytes().unwrap().as_slice(),
            expected.bytes().unwrap().as_slice(),
            "{browser}: restore readback mismatch"
        );
        let exported = browser_session::export_profile(browser, &profile).expect(browser);
        assert_eq!(
            exported.bytes().unwrap().as_slice(),
            expected.bytes().unwrap().as_slice(),
            "{browser}: later export mismatch"
        );
        assert_eq!(
            std::fs::read(profile.join("sentinel")).unwrap(),
            b"existing-browser-data"
        );
        eprintln!("{browser}: large IndexedDB restore/export and existing profile sentinel passed");
    }
}
