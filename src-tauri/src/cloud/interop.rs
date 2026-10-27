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
        assert_eq!(a.disk.accounts[&a.account_id().unwrap()].bindings[&p.id].generation,0,
            "a closed writer must release its lease after upload");
        super::super::session_sync::prepare_launch(&mut b, &pb.id).unwrap();
        sync::run(&mut b, &runtime).unwrap();
        assert_eq!(b.disk.accounts[&b.account_id().unwrap()].bindings[&pb.id].generation,0,
            "a device that prepared but closed the profile must release its lease");
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
        assert!(profiles::remove(&r2, &copied.id, true, &runtime).is_err(),
            "pending deletion must retain its explicit local tombstone");
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
        profiles::remove(&r2, &copied.id, true, &runtime).unwrap();
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

    #[test]
    #[ignore = "requires isolated Go/PostgreSQL harness and test-only OS credentials"]
    fn desktop_two_device_config_sync_lifecycle() {
        let dir = std::path::PathBuf::from(std::env::var("DESKTOP_INTEROP_DIR").unwrap());
        let endpoint = std::fs::read_to_string(dir.join("endpoint")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let (r1,r2) = (temp.path().join("one"),temp.path().join("two"));
        let namespaces = [format!("test-{}",uuid::Uuid::new_v4()),format!("test-{}",uuid::Uuid::new_v4())];
        struct Cleanup { namespaces:[String;2], account:Option<String>, dir:std::path::PathBuf }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                for ns in &self.namespaces {
                    let _ = vault::remove(&format!("{ns}:device"));
                    let _ = vault::remove(&format!("{ns}:session"));
                    if let Some(id) = &self.account {
                        let _ = vault::remove(&format!("{ns}:{id}:root:1"));
                        let _ = vault::clear_staged(ns,id,1);
                        let _ = vault::remove(&format!("{ns}:{id}:recovery-pending"));
                    }
                }
                let _ = std::fs::write(self.dir.join("done"), b"finished");
            }
        }
        let mut cleanup = Cleanup { namespaces:namespaces.clone(),account:None,dir:dir.clone() };
        let mut a = Connection::test_connection(&r1,Client::new(&endpoint,true).unwrap(),&namespaces[0]).unwrap();
        let mut b = Connection::test_connection(&r2,Client::new(&endpoint,true).unwrap(),&namespaces[1]).unwrap();
        login(&mut a,&dir);
        cleanup.account = Some(a.account_id().unwrap());
        let recovery = a.initialize().unwrap()["recoveryCode"].as_str().unwrap().to_owned();
        a.acknowledge_recovery().unwrap();login(&mut b,&dir);b.recover(&recovery).unwrap();
        let draft:profiles::Draft = serde_json::from_value(json!({"name":"Original","browser":"firefox","notes":"initial","tags":[],"proxy":{"mode":"direct","host":"","port":0,"username":""},"preferences":profiles::Preferences::default()})).unwrap();
        let runtime = profiles::Runtime::default();
        let p = profiles::create(&r1,draft.clone(),None).unwrap();
        sync::setup(&mut a,"upload",&[p.id.clone()],&[],false,&runtime).unwrap();
        let account = a.account_id().unwrap();
        let cloud = a.disk.accounts[&account].bindings[&p.id].cloud_id.clone();
        sync::setup(&mut b,"merge",&[],&[cloud.clone()],false,&runtime).unwrap();
        let peer = profiles::load(&r2).unwrap().into_iter().find(|x|x.name=="Original").unwrap();
        let marker = profiles::path(&r2,&peer).unwrap().join("sync-test-local-data");
        std::fs::write(&marker,b"preserved").unwrap();
        let revision = sync::fetch(&mut a,&cloud,"config").unwrap().revision;
        sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        assert_eq!(sync::fetch(&mut a,&cloud,"config").unwrap().revision,revision,"unchanged sync must not create writes");
        let mut local = super::super::model::Document::export(&profiles::get(&r1,&p.id,false).unwrap()).unwrap().draft();
        local.name="Renamed".into();profiles::update(&r1,&p.id,local).unwrap();
        let mut remote = super::super::model::Document::export(&profiles::get(&r2,&peer.id,false).unwrap()).unwrap().draft();
        remote.notes="Independent note".into();profiles::update(&r2,&peer.id,remote).unwrap();
        sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();sync::run(&mut a,&runtime).unwrap();
        let merged=profiles::get(&r1,&p.id,false).unwrap();assert_eq!(merged.name,"Renamed");assert_eq!(merged.notes,"Independent note");
        for choice in ["local","cloud","both"] {
            sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
            let mut da=super::super::model::Document::export(&profiles::get(&r1,&p.id,false).unwrap()).unwrap().draft();
            let mut db=super::super::model::Document::export(&profiles::get(&r2,&peer.id,false).unwrap()).unwrap().draft();
            da.name=format!("Remote {choice}");db.name=format!("Local {choice}");
            profiles::update(&r1,&p.id,da).unwrap();profiles::update(&r2,&peer.id,db).unwrap();
            sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
            assert!(b.disk.accounts[&account].conflicts.iter().any(|x|x.local_id==peer.id&&x.fields.contains(&"/name".into())));
            let before=sync::fetch(&mut b,&cloud,"config").unwrap().revision;
            sync::resolve(&mut b,&peer.id,"skip",&runtime).unwrap();
            assert_eq!(sync::fetch(&mut b,&cloud,"config").unwrap().revision,before);
            sync::resolve(&mut b,&peer.id,choice,&runtime).unwrap();
            let kept=profiles::get(&r2,&peer.id,false).unwrap();
            assert_eq!(kept.name,format!("{} {choice}",if choice=="local"{"Local"}else{"Remote"}));
            if choice=="both" { assert!(profiles::load(&r2).unwrap().iter().any(|x|x.name=="Local both (copy)")); }
            assert_eq!(std::fs::read(&marker).unwrap(),b"preserved");
        }
        // An unavailable service must preserve pending local changes, which the
        // next successful sync can publish without resetting the local profile.
        let mut offline_draft=super::super::model::Document::export(&profiles::get(&r2,&peer.id,false).unwrap()).unwrap().draft();
        offline_draft.notes="Offline change retained".into();profiles::update(&r2,&peer.id,offline_draft).unwrap();
        let registry_before=std::fs::read(r2.join("profiles.json")).unwrap();
        let state_before=std::fs::read(r2.join("cloud/state.json")).unwrap();
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let offline_endpoint=format!("http://{}",listener.local_addr().unwrap());drop(listener);
        let mut offline=Connection::test_connection(&r2,Client::new(&offline_endpoint,true).unwrap(),&namespaces[1]).unwrap();
        assert!(sync::run(&mut offline,&runtime).is_err());
        assert_eq!(std::fs::read(r2.join("profiles.json")).unwrap(),registry_before);
        assert_eq!(std::fs::read(r2.join("cloud/state.json")).unwrap(),state_before);
        sync::run(&mut b,&runtime).unwrap();sync::run(&mut a,&runtime).unwrap();
        assert_eq!(profiles::get(&r1,&p.id,false).unwrap().notes,"Offline change retained");
        sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        profiles::remove(&r1,&p.id,false,&runtime).unwrap();sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        assert!(profiles::get(&r2,&peer.id,false).is_ok(),"remote deletion must preserve the local profile pending choice");
        sync::resolve(&mut b,&peer.id,"local",&runtime).unwrap();
        assert!(!sync::fetch(&mut b,&cloud,"config").unwrap().deleted);
        sync::run(&mut a,&runtime).unwrap();assert!(profiles::get(&r1,&p.id,false).is_ok());
        profiles::remove(&r1,&p.id,false,&runtime).unwrap();sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        sync::resolve(&mut b,&peer.id,"cloud",&runtime).unwrap();
        assert!(profiles::get(&r2,&peer.id,true).unwrap().deleted_at.is_some());
        assert_eq!(std::fs::read(&marker).unwrap(),b"preserved");
        profiles::remove(&r2,&peer.id,true,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        assert!(!b.disk.accounts[&account].conflicts.iter().any(|x|x.local_id==peer.id));
        let missing=profiles::create(&r1,draft,None).unwrap();sync::run(&mut a,&runtime).unwrap();sync::run(&mut b,&runtime).unwrap();
        let missing_cloud=a.disk.accounts[&account].bindings[&missing.id].cloud_id.clone();
        let missing_peer=profiles::load(&r2).unwrap().into_iter().find(|x|x.name=="Original"&&x.deleted_at.is_none()).unwrap();
        let missing_marker=profiles::path(&r2,&missing_peer).unwrap().join("sync-test-preserved");std::fs::write(&missing_marker,b"preserved").unwrap();
        let before=sync::fetch(&mut b,&missing_cloud,"config").unwrap().revision;
        let file=r2.join("profiles.json");let mut registry:serde_json::Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        registry["profiles"].as_array_mut().unwrap().retain(|x|x["id"]!=missing_peer.id);
        std::fs::write(&file,serde_json::to_vec(&registry).unwrap()).unwrap();
        sync::run(&mut b,&runtime).unwrap();
        assert!(b.disk.accounts[&account].conflicts.iter().any(|x|x.local_id==missing_peer.id&&x.fields==["local_config_missing"]));
        assert_eq!(b.disk.accounts[&account].conflicts.iter().find(|x|x.local_id==missing_peer.id).unwrap().name,"Original",
            "a missing registry entry must retain its known cloud name instead of claiming deletion");
        assert!(sync::resolve(&mut b,&missing_peer.id,"local",&runtime).is_err());
        let after=sync::fetch(&mut b,&missing_cloud,"config").unwrap();assert_eq!(after.revision,before);assert!(!after.deleted);
        assert_eq!(std::fs::read(&missing_marker).unwrap(),b"preserved");
        // Old session bookkeeping cannot turn an empty recovered directory into
        // a user edit that overwrites the cloud login data on the next sync.
        let mut checkpoint=after.clone();checkpoint.kind="session".into();
        let binding=b.disk.accounts.get_mut(&account).unwrap().bindings.get_mut(&missing_peer.id).unwrap();
        binding.session_baseline=Some(checkpoint.clone());binding.session_hash=Some("old-local-session".into());
        binding.file_stamp=Some("old-local-files".into());binding.session_dirty=true;binding.generation=17;
        b.disk.save(&r2).unwrap();
        // A failed materialization must already have durably fenced the old
        // comparison state, before any recovered registry can become visible.
        let settings=profiles::path(&r2,&missing_peer).unwrap().join("nodecloak-runtime-settings.json");
        let previous_settings=std::fs::read(&settings).unwrap();
        std::fs::remove_file(&settings).unwrap();std::fs::create_dir(&settings).unwrap();
        assert!(sync::resolve(&mut b,&missing_peer.id,"cloud",&runtime).is_err());
        let disk=Disk::load(&r2).unwrap();let durable=&disk.accounts[&account].bindings[&missing_peer.id];
        assert!(durable.session_baseline.is_none(),"session comparisons must be durably reset before materializing recovered config");
        assert_eq!(durable.session_checkpoint.as_ref().unwrap().revision,checkpoint.revision);
        assert!(profiles::get(&r2,&missing_peer.id,false).is_err());
        assert_eq!(std::fs::read(&missing_marker).unwrap(),b"preserved");
        std::fs::remove_dir(&settings).unwrap();std::fs::write(&settings,previous_settings).unwrap();
        sync::resolve(&mut b,&missing_peer.id,"cloud",&runtime).unwrap();
        let recovered=profiles::get(&r2,&missing_peer.id,false).unwrap();assert_eq!(recovered.name,"Original");
        assert_eq!(std::fs::read(&missing_marker).unwrap(),b"preserved");
        let binding=&b.disk.accounts[&account].bindings[&missing_peer.id];
        assert!(binding.session_baseline.is_none());assert_eq!(binding.session_checkpoint.as_ref().unwrap().revision,checkpoint.revision);
        assert!(binding.session_hash.is_none()&&binding.file_stamp.is_none()&&!binding.session_dirty);assert_eq!(binding.generation,0);
        assert!(!b.disk.accounts[&account].conflicts.iter().any(|x|x.local_id==missing_peer.id));
        assert_eq!(sync::fetch(&mut b,&missing_cloud,"config").unwrap().revision,before,"recovery never writes a cloud configuration");
        eprintln!("config lifecycle passed: initial sync, unchanged sync, independent merge, same-field local/cloud/both/skip, offline retry, deletion restore/accept/purge, missing registry protection");
    }
    #[test]
    #[ignore="Requires isolated Go/PostgreSQL harness, OS credential store and installed Firefox"]
    fn desktop_two_device_large_session_sync(){
        let dir=std::path::PathBuf::from(std::env::var("DESKTOP_INTEROP_DIR").unwrap());
        let endpoint=std::fs::read_to_string(dir.join("endpoint")).unwrap();
        let temp=tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let (r1,r2)=(temp.path().join("one"),temp.path().join("two"));
        let namespaces=[format!("test-{}",uuid::Uuid::new_v4()),format!("test-{}",uuid::Uuid::new_v4())];
        struct Cleanup{ns:[String;2],account:Option<String>,dir:std::path::PathBuf}
        impl Drop for Cleanup{fn drop(&mut self){for ns in &self.ns{let _=vault::remove(&format!("{ns}:device"));let _=vault::remove(&format!("{ns}:session"));if let Some(a)=&self.account{let _=vault::remove(&format!("{ns}:{a}:root:1"));let _=vault::remove(&format!("{ns}:{a}:recovery-pending"));let _=vault::clear_staged(ns,a,1);}}let _=std::fs::write(self.dir.join("done"),b"finished");}}
        let mut cleanup=Cleanup{ns:namespaces.clone(),account:None,dir:dir.clone()};
        let mut a=Connection::test_connection(&r1,Client::new(&endpoint,true).unwrap(),&namespaces[0]).unwrap();
        let mut b=Connection::test_connection(&r2,Client::new(&endpoint,true).unwrap(),&namespaces[1]).unwrap();
        login(&mut a,&dir);let account=a.account_id().unwrap();cleanup.account=Some(account.clone());
        let recovery=a.initialize().unwrap()["recoveryCode"].as_str().unwrap().to_owned();a.acknowledge_recovery().unwrap();
        login(&mut b,&dir);b.recover(&recovery).unwrap();
        let runtime=profiles::Runtime::default();
        let draft:profiles::Draft=serde_json::from_value(json!({"name":"Large fixture","browser":"firefox","proxy":{"mode":"direct","host":"","port":0,"username":""},"preferences":profiles::Preferences::default()})).unwrap();
        let p=profiles::create(&r1,draft,None).unwrap();sync::setup(&mut a,"upload",&[p.id.clone()],&[],false,&runtime).unwrap();
        let cloud=a.disk.accounts[&account].bindings[&p.id].cloud_id.clone();
        sync::setup(&mut b,"merge",&[],&[cloud.clone()],false,&runtime).unwrap();
        let peer=profiles::load(&r2).unwrap().into_iter().find(|p|p.name=="Large fixture").unwrap();
        let expected=super::super::session_capacity_checks::fixture();
        super::super::browser_session::restore_profile("firefox",&profiles::path(&r1,&p).unwrap(),&expected).unwrap();
        for c in [&mut a,&mut b]{c.disk.accounts.get_mut(&account).unwrap().session_sync=true;c.disk.save(&c.root).unwrap();}
        sync::run(&mut a,&runtime).unwrap();let first=sync::fetch(&mut a,&cloud,"session").unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&first.payload).unwrap()["version"],2);
        assert!(first.payload.len()<expected.bytes().unwrap().len()/10);
        sync::run(&mut b,&runtime).unwrap();
        let actual=super::super::browser_session::export_profile("firefox",&profiles::path(&r2,&peer).unwrap()).unwrap();
        assert_eq!(actual.bytes().unwrap().as_slice(),expected.bytes().unwrap().as_slice());
        assert_eq!(sync::fetch(&mut b,&cloud,"session").unwrap().revision,first.revision,"receiving a large snapshot must not create another cloud write");
        assert!(b.disk.accounts[&account].conflicts.is_empty());
        eprintln!("Two devices: >8 MiB native snapshot, compressed encrypted v2 upload, full restore and unchanged remote revision passed");
    }
}
