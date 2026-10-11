#[cfg(test)]
mod tests {
    use super::super::{account::Connection, client::Client, model::Disk, sync, vault};
    use crate::profiles;
    use reqwest::Method;
    use serde_json::json;
    use std::path::Path;
    fn login(c: &mut Connection, dir: &Path) {
        c.client
            .call(
                Method::POST,
                "/v1/auth/email/request",
                None,
                Some(&json!({"email":"desktop-interop@example.com"})),
            )
            .unwrap();
        let code = std::fs::read_to_string(dir.join("code")).unwrap();
        let v=c.client.call(Method::POST,"/v1/auth/email/verify",None,Some(&json!({"email":"desktop-interop@example.com","code":code,"device":c.device.registration()}))).unwrap();
        c.finish_login(v).unwrap();
        c.refresh_vault().unwrap();
    }
    #[test]
    #[ignore = "requires real isolated Go/PostgreSQL harness and OS credential store"]
    fn desktop_real_server_crypto_merge_recovery_and_revocation() {
        let dir = std::path::PathBuf::from(
            std::env::var("DESKTOP_INTEROP_DIR").expect("isolated harness directory"),
        );
        let endpoint = std::fs::read_to_string(dir.join("endpoint")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let r1 = temp.path().join("one");
        let r2 = temp.path().join("two");
        let ns1 = format!("test-{}", uuid::Uuid::new_v4());
        let ns2 = format!("test-{}", uuid::Uuid::new_v4());
        let mut a =
            Connection::test_connection(&r1, Client::new(&endpoint, true).unwrap(), &ns1).unwrap();
        let mut b =
            Connection::test_connection(&r2, Client::new(&endpoint, true).unwrap(), &ns2).unwrap();
        login(&mut a, &dir);
        let init = a.initialize().unwrap();
        let recovery = init["recoveryCode"].as_str().unwrap().to_string();
        a.acknowledge_recovery().unwrap();
        login(&mut b, &dir);
        assert!(b.unlocked_root().is_err());
        if std::env::var("DESKTOP_INTEROP_APPROVAL").is_ok() {
            let id = b.account_id().unwrap();
            let rejected = super::super::crypto::random_key().unwrap();
            let tentative = super::super::crypto::random_key().unwrap();
            vault::stage_root(&ns2, &id, 1, &tentative).unwrap();
            vault::save_pending(&ns2, &id, 1, &rejected).unwrap();
            b.disk.accounts.get_mut(&id).unwrap().epoch = 1;
            b.disk.save(&r2).unwrap();
            let fp = super::super::account::fingerprint(&id, &b.device.id, &b.device.public_key);
            a.approve(&b.device.id, &fp[fp.len() - 12..]).unwrap();
            b.refresh_vault().unwrap();
            assert!(b.disk.accounts[&id].recovery_saved);
            assert!(b.unlocked_root().is_err());
            assert!(b.confirm_device("000000000000").is_err());
            let trusted_code = super::super::crypto::vault_code(&id, &a.unlocked_root().unwrap());
            b.confirm_device(&trusted_code).unwrap();
            assert!(b.pending_recovery().is_err());
        } else {
            b.recover(&recovery).unwrap();
            b.recover(&recovery).unwrap(); // Recovery also repairs an already-approved device.
        }
        assert_eq!(*a.unlocked_root().unwrap(), *b.unlocked_root().unwrap());
        let mut draft:profiles::Draft=serde_json::from_value(json!({"name":"Synthetic","browser":"firefox","notes":"original","tags":[],"proxy":{"mode":"direct","host":"","port":0,"username":""},"password":null,"clearPassword":false,"preferences":profiles::Preferences::default()})).unwrap();
        let p = profiles::create(&r1, draft.clone(), None).unwrap();
        let runtime = profiles::Runtime::default();
        sync::setup(&mut a, "upload", &[p.id.clone()], &[], false, &runtime).unwrap();
        let objects = sync::manifest(&mut b).unwrap();
        assert_eq!(objects.len(), 1);
        let cloud = objects[0].id.clone();
        sync::setup(&mut b, "merge", &[], &[cloud.clone()], false, &runtime).unwrap();
        let pb = profiles::load(&r2)
            .unwrap()
            .into_iter()
            .find(|p| p.name == "Synthetic")
            .unwrap();
        draft.name = "Edited name".into();
        profiles::update(&r1, &p.id, draft.clone()).unwrap();
        sync::run(&mut a, &runtime).unwrap();
        let mut d2 = super::super::model::Document::export(&pb).unwrap().draft();
        d2.notes = "Independent note".into();
        profiles::update(&r2, &pb.id, d2).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        sync::run(&mut a, &runtime).unwrap();
        let merged = profiles::get(&r1, &p.id, false).unwrap();
        assert_eq!(merged.name, "Edited name");
        assert_eq!(merged.notes, "Independent note");
        assert!(a.disk.accounts[&a.account_id().unwrap()]
            .conflicts
            .is_empty());
        let old_binding = b.disk.accounts[&b.account_id().unwrap()].bindings[&pb.id]
            .cloud_id
            .clone();
        sync::detach(&mut b, &pb.id, true, &runtime).unwrap();
        assert!(profiles::get(&r2, &pb.id, false).is_err());
        sync::setup(&mut b, "merge", &[], &[old_binding], false, &runtime).unwrap();
        assert_eq!(
            profiles::load(&r2)
                .unwrap()
                .iter()
                .filter(|p| p.name == "Edited name" && p.deleted_at.is_none())
                .count(),
            1
        );
        assert!(!b.disk.accounts[&b.account_id().unwrap()]
            .detached
            .contains(&pb.id));
        let expiry = chrono::Utc::now().timestamp() + 604800;
        let website_state = |value: &str| {
            super::super::browser_session::Snapshot::parse(json!({"schema":1,"cookies":[{"name":"synthetic_login","value":value,"domain":"cloud-fixture.invalid","path":"/","secure":true,"httpOnly":true,"sameSite":"Lax","expires":expiry}],"origins":[{"origin":"https://cloud-fixture.invalid","state":{"local":[["login",value]],"databases":[]}}]})).unwrap()
        };
        let apath = profiles::path(&r1, &merged).unwrap();
        let bpath = profiles::path(&r2, &pb).unwrap();
        let original = website_state("fixture-login-original");
        super::super::browser_session::restore_profile("firefox", &apath, &original).unwrap();
        for c in [&mut a, &mut b] {
            let id = c.account_id().unwrap();
            c.disk.accounts.get_mut(&id).unwrap().session_sync = true;
            c.disk.save(&c.root).unwrap();
        }
        sync::run(&mut a, &runtime).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        let old_session = sync::fetch(&mut b, &cloud, "session").unwrap();
        let downloaded = super::super::browser_session::export_profile("firefox", &bpath).unwrap();
        assert_eq!(
            downloaded.bytes().unwrap().as_slice(),
            original.bytes().unwrap().as_slice()
        );
        super::super::browser_session::restore_profile(
            "firefox",
            &apath,
            &website_state("fixture-login-a"),
        )
        .unwrap();
        super::super::browser_session::restore_profile(
            "firefox",
            &bpath,
            &website_state("fixture-login-b"),
        )
        .unwrap();
        sync::run(&mut a, &runtime).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        assert!(b.disk.accounts[&b.account_id().unwrap()]
            .conflicts
            .iter()
            .any(|x| x.kind == "session"));
        super::super::session_sync::resolve(&mut b, &pb.id, "both", &runtime).unwrap();
        assert!(profiles::load(&r2)
            .unwrap()
            .iter()
            .any(|p| p.name.ends_with("(copy)")));
        let retained = profiles::load(&r2)
            .unwrap()
            .into_iter()
            .find(|p| p.name.ends_with("(copy)"))
            .unwrap();
        assert_eq!(
            super::super::browser_session::export_profile(
                "firefox",
                &profiles::path(&r2, &retained).unwrap()
            )
            .unwrap()
            .cookies[0]
                .value,
            "fixture-login-b"
        );
        let reconciled = super::super::browser_session::export_profile("firefox", &bpath).unwrap();
        assert_eq!(reconciled.cookies[0].value, "fixture-login-a");
        let account = b.account_id().unwrap();
        let mut confirmed = b.disk.accounts[&account].bindings[&pb.id].clone();
        let before = serde_json::to_value(&confirmed).unwrap();
        assert!(confirmed.session_baseline.as_ref().unwrap().revision > old_session.revision);
        assert!(
            super::super::session_sync::one(&mut b, &pb, &mut confirmed, Some(&old_session))
                .unwrap_err()
                .contains("版本倒退")
        );
        assert_eq!(before, serde_json::to_value(&confirmed).unwrap());
        let original_binding = confirmed.clone();
        confirmed.session_checkpoint = confirmed.session_baseline.take();
        b.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(pb.id.clone(), confirmed);
        b.disk.save(&r2).unwrap();
        b.disk = Disk::load(&r2).unwrap();
        let mut recreated = b.disk.accounts[&account].bindings[&pb.id].clone();
        assert!(
            super::super::session_sync::one(&mut b, &pb, &mut recreated, Some(&old_session))
                .unwrap_err()
                .contains("版本倒退")
        );
        b.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(pb.id.clone(), original_binding);
        b.disk.save(&r2).unwrap();

        assert_eq!(
            super::super::browser_session::export_profile("firefox", &bpath)
                .unwrap()
                .bytes()
                .unwrap()
                .as_slice(),
            reconciled.bytes().unwrap().as_slice()
        );

        sync::run(&mut b, &runtime).unwrap();
        let copied = profiles::load(&r2)
            .unwrap()
            .into_iter()
            .find(|p| p.name.ends_with("(copy)"))
            .unwrap();
        let copied_cloud = b.disk.accounts[&b.account_id().unwrap()].bindings[&copied.id]
            .cloud_id
            .clone();
        profiles::remove(&r2, &copied.id, false, &runtime).unwrap();
        profiles::remove(&r2, &copied.id, true, &runtime).unwrap();
        let mut other_lease = super::super::model::Binding::new(&copied_cloud);
        super::super::session_sync::lease(&mut a, &mut other_lease, true).unwrap();
        let mut live_doc =
            super::super::model::Document::export(&profiles::get(&r2, &pb.id, false).unwrap())
                .unwrap();
        live_doc.notes = "Survives blocked deletion".into();
        profiles::update(&r2, &pb.id, live_doc.draft()).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        let account = b.account_id().unwrap();
        assert!(b.disk.accounts[&account].conflicts.iter().any(
            |x| x.local_id == copied.id && x.fields.contains(&"deletion_waiting_device".into())
        ));
        let active = sync::fetch(&mut b, &cloud, "config").unwrap();
        assert_eq!(
            sync::document(&b, &active).unwrap().notes,
            "Survives blocked deletion"
        );
        super::super::session_sync::lease(&mut b, &mut other_lease, true).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        assert!(!b.disk.accounts[&account]
            .conflicts
            .iter()
            .any(|x| x.local_id == copied.id && x.kind == "session"));
        assert!(
            sync::fetch(&mut b, &copied_cloud, "config")
                .unwrap()
                .deleted
        );
        let removed_session = sync::fetch(&mut b, &copied_cloud, "session").unwrap();
        assert!(removed_session.deleted);
        let plain = sync::decrypt(&b, &removed_session).unwrap();
        let cleared =
            super::super::browser_session::Snapshot::parse(serde_json::from_slice(&plain).unwrap())
                .unwrap();
        assert!(cleared.cookies.is_empty() && cleared.origins.is_empty());
        // Config deletion can arrive while session deletion is blocked by this
        // device's lease. Launch must not publish the offline session in that gap.
        let mut blocked_draft = draft.clone();
        blocked_draft.name = "Launch during blocked deletion".into();
        let blocked = profiles::create(&r1, blocked_draft, None).unwrap();
        let blocked_login = website_state("held-lease-login");
        super::super::browser_session::restore_profile(
            "firefox",
            &profiles::path(&r1, &blocked).unwrap(),
            &blocked_login,
        )
        .unwrap();
        sync::run(&mut a, &runtime).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        let peer = profiles::load(&r2)
            .unwrap()
            .into_iter()
            .find(|p| p.name == blocked.name)
            .unwrap();
        let mut binding = b.disk.accounts[&account].bindings[&peer.id].clone();
        super::super::session_sync::lease(&mut b, &mut binding, true).unwrap();
        b.disk
            .accounts
            .get_mut(&account)
            .unwrap()
            .bindings
            .insert(peer.id.clone(), binding.clone());
        let session_before = sync::fetch(&mut b, &binding.cloud_id, "session").unwrap();
        let offline_login = website_state("held-lease-offline-edit");
        super::super::browser_session::restore_profile(
            "firefox",
            &profiles::path(&r2, &peer).unwrap(),
            &offline_login,
        )
        .unwrap();
        let mut changed = super::super::model::Document::export(&peer)
            .unwrap()
            .draft();
        changed.notes = "Offline edit during deletion".into();
        profiles::update(&r2, &peer.id, changed).unwrap();
        profiles::remove(&r1, &blocked.id, false, &runtime).unwrap();
        sync::run(&mut a, &runtime).unwrap();
        assert!(
            sync::fetch(&mut b, &binding.cloud_id, "config")
                .unwrap()
                .deleted
        );
        assert!(
            !sync::fetch(&mut b, &binding.cloud_id, "session")
                .unwrap()
                .deleted
        );
        assert!(
            super::super::session_sync::prepare_launch(&mut b, &peer.id).is_err(),
            "launch must block before publishing into a config deletion"
        );
        assert_eq!(
            sync::fetch(&mut b, &binding.cloud_id, "session")
                .unwrap()
                .revision,
            session_before.revision
        );
        assert_eq!(
            super::super::browser_session::export_profile(
                "firefox",
                &profiles::path(&r2, &peer).unwrap()
            )
            .unwrap()
            .bytes()
            .unwrap(),
            offline_login.bytes().unwrap()
        );
        sync::run(&mut b, &runtime).unwrap();
        sync::resolve(&mut b, &peer.id, "local", &runtime).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        // A config deletion must never erase an active device's login data before
        // its independent edit has been resolved, including the launch-only path.
        for choice in ["local", "cloud", "both"] {
            let mut d = draft.clone();
            d.name = format!("Deletion conflict {choice}");
            let pa = profiles::create(&r1, d, None).unwrap();
            let path_a = profiles::path(&r1, &pa).unwrap();
            let login_state = website_state(&format!("preserve-{choice}"));
            super::super::browser_session::restore_profile("firefox", &path_a, &login_state)
                .unwrap();
            sync::run(&mut a, &runtime).unwrap();
            sync::run(&mut b, &runtime).unwrap();
            let peer = profiles::load(&r2)
                .unwrap()
                .into_iter()
                .find(|x| x.name == pa.name)
                .unwrap();
            let path_b = profiles::path(&r2, &peer).unwrap();
            let cloud_id = b.disk.accounts[&account].bindings[&peer.id]
                .cloud_id
                .clone();
            let mut edited = super::super::model::Document::export(&peer)
                .unwrap()
                .draft();
            edited.notes = "Offline config edit".into();
            profiles::update(&r2, &peer.id, edited).unwrap();
            profiles::remove(&r1, &pa.id, false, &runtime).unwrap();
            sync::run(&mut a, &runtime).unwrap();
            let deleted_session = sync::fetch(&mut b, &cloud_id, "session").unwrap();
            assert!(deleted_session.deleted);
            let mut binding = b.disk.accounts[&account].bindings[&peer.id].clone();
            super::super::session_sync::one(&mut b, &peer, &mut binding, Some(&deleted_session))
                .unwrap();
            assert_eq!(
                super::super::browser_session::export_profile("firefox", &path_b)
                    .unwrap()
                    .bytes()
                    .unwrap(),
                login_state.bytes().unwrap(),
                "launch must preserve login state before config sync"
            );
            b.disk
                .accounts
                .get_mut(&account)
                .unwrap()
                .bindings
                .insert(peer.id.clone(), binding);
            sync::run(&mut b, &runtime).unwrap();
            assert!(b.disk.accounts[&account]
                .conflicts
                .iter()
                .any(|x| x.local_id == peer.id && x.kind == "config"));
            assert_eq!(
                super::super::browser_session::export_profile("firefox", &path_b)
                    .unwrap()
                    .bytes()
                    .unwrap(),
                login_state.bytes().unwrap()
            );
            sync::resolve(&mut b, &peer.id, "skip", &runtime).unwrap();
            sync::run(&mut b, &runtime).unwrap();
            assert_eq!(
                super::super::browser_session::export_profile("firefox", &path_b)
                    .unwrap()
                    .bytes()
                    .unwrap(),
                login_state.bytes().unwrap(),
                "skip must remain non-destructive"
            );
            assert!(
                super::super::session_sync::resolve(&mut b, &peer.id, "cloud", &runtime).is_err(),
                "session choice cannot bypass unresolved config conflict"
            );
            sync::resolve(&mut b, &peer.id, choice, &runtime).unwrap();
            sync::run(&mut b, &runtime).unwrap();
            if choice == "local" {
                assert_eq!(
                    super::super::browser_session::export_profile("firefox", &path_b)
                        .unwrap()
                        .bytes()
                        .unwrap(),
                    login_state.bytes().unwrap()
                );
                assert!(b.disk.accounts[&account]
                    .conflicts
                    .iter()
                    .any(|x| x.local_id == peer.id && x.kind == "session"));
                super::super::session_sync::resolve(&mut b, &peer.id, "local", &runtime).unwrap();
                assert!(!sync::fetch(&mut b, &cloud_id, "session").unwrap().deleted);
            } else {
                assert!(profiles::get(&r2, &peer.id, true)
                    .unwrap()
                    .deleted_at
                    .is_some());
                if choice == "both" {
                    let preserved = profiles::load(&r2)
                        .unwrap()
                        .into_iter()
                        .find(|x| x.name == format!("{} (copy)", peer.name))
                        .unwrap();
                    assert_eq!(
                        super::super::browser_session::export_profile(
                            "firefox",
                            &profiles::path(&r2, &preserved).unwrap()
                        )
                        .unwrap()
                        .bytes()
                        .unwrap(),
                        login_state.bytes().unwrap()
                    );
                }
            }
        }
        let oid = a.disk.accounts[&a.account_id().unwrap()].bindings[&p.id]
            .cloud_id
            .clone();
        let encrypted = sync::fetch(&mut a, &oid, "session").unwrap();
        assert!(!encrypted.payload.contains("fixture-login"));
        assert!(!std::fs::read_to_string(r1.join("cloud/state.json"))
            .unwrap()
            .contains("fixture-login"));
        let cleanup_account = a.account_id().unwrap();
        let old_epoch = a.disk.accounts[&a.account_id().unwrap()].epoch;
        let new_recovery = super::super::rotation::revoke(&mut a, &b.device.id).unwrap();
        assert!(new_recovery["recoveryCode"].as_str().unwrap() != recovery);
        assert_eq!(
            a.disk.accounts[&a.account_id().unwrap()].epoch,
            old_epoch + 1
        );
        assert!(b.request(Method::GET, "/v1/objects", None).is_err());
        a.acknowledge_recovery().unwrap();
        sync::run(&mut a, &runtime).unwrap();
        let disk = std::fs::read_to_string(r1.join("cloud/state.json")).unwrap();
        assert!(!disk.contains(&recovery));
        assert!(!disk.contains("accessToken"));
        let _ = Disk::load(&r1).unwrap();
        for ns in [&ns1, &ns2] {
            let _ = vault::remove(&format!("{ns}:device"));
            let _ = vault::remove(&format!("{ns}:session"));
            let id = &cleanup_account;
            {
                for epoch in 1..=2 {
                    let _ = vault::remove(&format!("{ns}:{id}:root:{epoch}"));
                    let _ = vault::clear_staged(ns, id, epoch);
                }
                let _ = vault::remove(&format!("{ns}:{id}:recovery-pending"));
            }
        }
        std::fs::write(dir.join("done"), b"passed").unwrap();
    }
}
