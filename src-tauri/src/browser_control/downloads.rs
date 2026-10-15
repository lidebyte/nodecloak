//! A persistent browser context has one download policy. Keep that policy bound to
//! one client until its pages (including popups) and downloads have finished.
use super::*;
use crate::mcp::policy::Grant;

pub(super) struct Lease {
    owner: String,
    directory: Option<PathBuf>,
    expires: i64,
    tabs: HashSet<String>,
    pending: HashSet<String>,
    revoked: bool,
    denied: bool,
}
impl BrowserControl {
    pub(super) fn download_policy_after_reconnect(&mut self) -> Result<(), String> {
        self.download_policy(None, false)
    }
    fn download_policy(&mut self, directory: Option<&Path>, restore: bool) -> Result<(), String> {
        if self.firefox {
            let behavior = if restore {
                Value::Null
            } else if let Some(path) = directory {
                json!({"type":"allowed","destinationFolder":automation::browser_path(path)})
            } else {
                json!({"type":"denied"})
            };
            self.protocol.call(
                "browser.setDownloadBehavior",
                json!({"downloadBehavior":behavior,"userContexts":["default"]}),
                None,
            )?;
        } else {
            let params = if restore {
                json!({"behavior":"default","eventsEnabled":true})
            } else if let Some(path) = directory {
                json!({"behavior":"allow","downloadPath":automation::browser_path(path),"eventsEnabled":true})
            } else {
                json!({"behavior":"deny","eventsEnabled":true})
            };
            self.protocol
                .call("Browser.setDownloadBehavior", params, None)?;
        }
        Ok(())
    }
    pub(super) fn claim_downloads(
        &mut self,
        g: &Grant,
        profile: &str,
        tab: &str,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        self.maintain_downloads(cancel)?;
        let permitted = g.permissions.iter().any(|p| p == "files") && !g.file_roots.is_empty();
        let directory = if permitted {
            Some(automation::download_dir(g, profile)?)
        } else {
            None
        };
        if let Some(lease) = &mut self.download_lease {
            if lease.owner != g.id || lease.directory != directory || lease.revoked || lease.denied
            {
                return Err("DOWNLOAD_CONTEXT_IN_USE: finish downloads and close the previous client's task tabs before using another client or file policy".into());
            }
            lease.expires = lease.expires.min(g.expires_at);
            lease.tabs.insert(tab.into());
            return Ok(());
        }
        if self.firefox && !self.download_subscribed {
            self.protocol.call("session.subscribe",json!({"events":["browsingContext.downloadWillBegin","browsingContext.downloadEnd"]}),None)?;
            self.download_subscribed = true;
        }
        self.download_policy(directory.as_deref(), false)?;
        self.download_lease = Some(Lease {
            owner: g.id.clone(),
            directory,
            expires: g.expires_at,
            tabs: HashSet::from([tab.into()]),
            pending: HashSet::new(),
            revoked: false,
            denied: false,
        });
        Ok(())
    }
    pub(super) fn download_event(&mut self, event: &Value) {
        let Some(lease) = &mut self.download_lease else {
            return;
        };
        let method = event["method"].as_str().unwrap_or("");
        let params = &event["params"];
        if [
            "Browser.downloadWillBegin",
            "browsingContext.downloadWillBegin",
        ]
        .contains(&method)
        {
            if let Some(id) = params["guid"].as_str().or(params["navigation"].as_str()) {
                lease.pending.insert(id.into());
            }
        } else if method == "browsingContext.downloadEnd"
            || (method == "Browser.downloadProgress"
                && ["completed", "canceled"].contains(&params["state"].as_str().unwrap_or("")))
        {
            if let Some(id) = params["guid"].as_str().or(params["navigation"].as_str()) {
                lease.pending.remove(id);
            }
        }
        let (id, opener) = if method == "Target.attachedToTarget" {
            (
                params["targetInfo"]["targetId"].as_str(),
                params["targetInfo"]["openerId"].as_str(),
            )
        } else if method == "browsingContext.contextCreated" {
            (
                params["context"].as_str(),
                params["originalOpener"].as_str(),
            )
        } else {
            (None, None)
        };
        if let (Some(id), Some(opener)) = (id, opener) {
            if lease.tabs.contains(opener) {
                lease.tabs.insert(id.into());
            }
        }
    }
    pub(super) fn invalidate_downloads(&mut self, client: Option<&str>) -> Result<(), String> {
        if self
            .download_lease
            .as_ref()
            .is_some_and(|l| !l.revoked && client.is_none_or(|id| l.owner == id))
        {
            self.download_lease.as_mut().unwrap().revoked = true;
            self.download_policy(None, false)?;
            self.download_lease.as_mut().unwrap().denied = true;
        }
        Ok(())
    }
    pub(super) fn finish_closed_download_tab(
        &mut self,
        tab: &str,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            self.maintain_downloads(cancel)?;
            if !self
                .download_lease
                .as_ref()
                .is_some_and(|l| l.tabs.contains(tab))
                || Instant::now() >= until
            {
                return Ok(());
            }
            // Closing is acknowledged before some browser versions retire the target.
            // Wait for that observable transition so a following manual download does
            // not race the controller's periodic default-policy restoration.
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    pub(super) fn maintain_downloads(&mut self, cancel: &AtomicBool) -> Result<(), String> {
        if self.download_lease.is_none() {
            return Ok(());
        }
        if self.download_lease.as_ref().is_some_and(|l| {
            !l.denied && (l.revoked || l.expires <= chrono::Utc::now().timestamp())
        }) {
            self.download_policy(None, false)?;
            self.download_lease.as_mut().unwrap().denied = true;
        }
        if !self.automation_dialogs.is_empty() {
            return Ok(());
        }
        let tree = self.tree(cancel)?;
        let lease = self.download_lease.as_mut().unwrap();
        // Discover popups before removing a closed opener. The event path also records
        // children created and detached between maintenance calls.
        loop {
            let before = lease.tabs.len();
            for item in &tree {
                if item["opener"]
                    .as_str()
                    .is_some_and(|id| lease.tabs.contains(id))
                {
                    if let Some(id) = item["tab_id"].as_str() {
                        lease.tabs.insert(id.into());
                    }
                }
            }
            if before == lease.tabs.len() {
                break;
            }
        }
        lease.tabs.retain(|id| {
            tree.iter()
                .any(|v| v["top_level"] == true && v["tab_id"] == *id)
        });
        if lease.tabs.is_empty() && lease.pending.is_empty() {
            self.download_policy(None, true)?;
            self.download_lease = None;
        } else if !lease.denied
            && (lease.revoked || lease.expires <= chrono::Utc::now().timestamp())
        {
            self.download_policy(None, false)?;
            self.download_lease.as_mut().unwrap().denied = true;
        }
        Ok(())
    }
}
impl Runtime {
    pub(crate) fn invalidate_downloads(&self, client: Option<&str>) {
        if let Ok(controls) = self.0.lock() {
            for control in controls.values().filter(|c| c.automation) {
                let _ = control
                    .sender
                    .send(Request::InvalidateDownloads(client.map(str::to_owned)));
            }
        }
    }
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
