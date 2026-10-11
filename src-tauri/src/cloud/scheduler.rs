use super::{account::Connection, model::Disk, sync};
use crate::{data_root, profiles, OperationLock};
use tauri::{Emitter, Manager};
pub fn start(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("nodecloak-cloud-sync".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let root = &app.state::<data_root::DataRoot>().0;
            // No credential-store access or network activity for an offline-only installation.
            let Ok(disk) = Disk::load(root) else { continue };
            let Some(account) = disk.account else {
                continue;
            };
            let Some(state) = disk.accounts.get(&account.id) else {
                continue;
            };
            if !state.auto_sync {
                continue;
            };
            let guard = app.state::<OperationLock>();
            let Ok(_lock) = guard.0.try_lock() else {
                continue;
            };
            let Ok(mut c) = Connection::load(root) else {
                continue;
            };
            let Ok(current) = c.account_id() else {
                continue;
            };
            if !c
                .disk
                .accounts
                .get(&current)
                .is_some_and(|s| s.auto_sync && s.setup_complete)
            {
                continue;
            };
            if let Err(error) = sync::run(&mut c, &app.state::<profiles::Runtime>()) {
                if let Ok(id) = c.account_id() {
                    if let Some(s) = c.disk.accounts.get_mut(&id) {
                        s.last_error = Some(error)
                    };
                    let _ = c.disk.save(root);
                }
            }
            let _ = app.emit("cloud-changed", ());
            let _ = app.emit("profiles-changed", ());
        })
        .expect("cloud scheduler thread");
}
