# Firefox local launch blocked by isolated session capture

## Evidence

On installed Firefox 158, a staged copy of a locally available profile reproduced an unsupported-operation/system-access rejection on accounts.firefox.com. Subsequent filesystem-handle verification established that this profile came from Codex's redirected AppData cache, not the installed application's physical AppData directory; this reproduction is not acceptance evidence for the installed user's exact profile. Browser::site discarded every evaluation error and eventually reported only a generic isolated-storage timeout. Firefox documents accounts.firefox.com and addons.mozilla.org as privileged Mozilla web content: https://firefox-source-docs.mozilla.org/dom/ipc/process_model.html#privileged-mozilla-content .

A new synthetic installed-browser regression failed with the same timeout. Configuring the isolated clone to use ordinary content for Mozilla web domains made its localStorage import/export roundtrip pass. All 26 discovered namespaces in the redirected profile copy then completed navigation. No website URLs or login values were logged by the diagnostic.

Full capture exposed a second independent blocker: the codec returned NC_LIMIT for existing website data. The old generic exception text could not distinguish this from unsupported data, and the owner-local startup policy rejected it.

## Repair boundaries

Only Stage::new's offline Firefox clone receives separatedMozillaDomains="" and separatePrivilegedMozillaWebContentProcess=false. All requested documents are network-intercepted empty HTML and a dead proxy remains configured. No system-access capability or browser-wide runtime flag is granted. Original user.js and prefs.js bytes are restored before commit; export never commits the clone.

System-access rejection now has a fixed actionable error instead of being swallowed by site readiness polling. A codec envelope retains only allowlisted NC_LIMIT/unsupported/non-exportable/storage-blocked identifiers, with no raw browser exception or database payload. Import and export retain staged verification; no partial snapshot is treated as complete.

Owner-local launch may continue after these explicit capture-unavailable failures, preserving the synchronization warning. Existing remote configuration checks, conflicts and sharing ownership gates remain in place. LocalOnly returns before session hashes/baselines/upload/restore, and the dirty flag and zero lease generation preserve work for later resolution. Manual sync and received shares retain fail-closed behavior.

## Verification

- Synthetic protected Mozilla web-storage regression failed before repair and passed afterward.
- Original user.js and prefs.js byte restoration and explicit >1MiB storage-limit reporting are covered by the installed Firefox regression.
- Owner-only/allow-local policy regression failed before classification changes and passed afterward; negative tests retain graph/config/lease errors and shares/manual-sync failures.
- Full ordinary Rust suite: 224 passed, 32 opt-in ignored. Frontend: 161 passed; production TypeScript/Vite build passed.
- Installed Firefox partitioned Cookie, LocalStorage and IndexedDB migration/isolated deletion passed. The static desktop debug executable built successfully at the configured local target; loading it awaits exiting the single installed instance. Installed Chrome and Edge partitioned Cookie, LocalStorage and IndexedDB migration/isolated deletion also passed. The final ordinary Rust executable run passed all 224 tests (32 opt-in ignored).

The redirected profile copy still contains data above codec limits; that condition is retained as a sync warning rather than discarded, truncated or uploaded partially. Current installed-user application acceptance remains pending on the physical application data. No live profile, cloud conflict, binding or website data was changed by diagnostic or conflict-resolution operations.

The repaired embedded-frontend local executable is built under the configured debug target (SHA-256 F581A6095E8D40141E5EA71F62BA0FE3C25CDC2A748AB02930F56E0981D3F86A). The first launch from Codex inherited its Windows package AppData redirection. GetFinalPathNameByHandle confirmed the redirected cache; a read-only WMI-created helper confirmed that physical AppData contains four profiles including live acsisters, whereas the redirected registry contains twenty entries without acsisters. Thus absence in the first repaired application's list did not establish loss of the installed configuration. A restart through the verified nonredirected process-launch route is pending releasing the single-instance lock. No installed binary was overwritten, and no release/commit/push was performed for this follow-up repair.
