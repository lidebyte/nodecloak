# Overall synchronization validation — 2026-10-16

## Scope

Windows local validation used a new disposable PostgreSQL 17 container bound to loopback, the real Go test service, unique native test credential namespaces, and temporary synthetic browser profiles. Tests did not operate on the user's production account or browser profiles. Shared-database tests ran serially.

## Results

| User flow | Result and evidence |
| --- | --- |
| First upload, second-device download, unchanged retry | Passed through real native clients and Go/PostgreSQL. An unchanged retry does not increment configuration revision. |
| Independent edits | Passed. A name change and note change merge across two devices. |
| Concurrent edits to the same field | Passed. Conflict preserves both states; skip sends no write; local/cloud/both choices produce the selected state. |
| Delete, keep local, restore | Passed after correcting repeated restoration conflict. A restored live remote now reaches the device whose deletion was already acknowledged. |
| Accept cloud deletion and purge | Passed. Local data remains in trash until an explicit decision; pending deletion retains its configuration record; acknowledged deletion can be purged without recreating a false conflict. |
| Missing local registry entry | Passed. Cloud revision/deleted flag stay unchanged; local sentinel file is preserved; keep-local cannot manufacture a deletion document. |
| Service unavailable, then retry | Passed. Registry and sync-state bytes remain unchanged after connection refusal; local edits publish successfully when the real test service is used again. |
| Login-state synchronization and conflicts | Passed through real Go/PostgreSQL, native credential storage, and synthetic Firefox session data. Includes keep-both preservation, deletion waiting on another device's lease, old encrypted-version rejection, recovery and revocation. |
| Chrome, Edge, Firefox storage | Passed with installed browsers. Partitioned Cookie, LocalStorage and IndexedDB migration, readback and isolated deletion all passed using temporary synthetic profiles. |
| Recovery discovery and completion | Passed. Row/launch/batch/save/restart/overview recovery links target the right profile; errors persist; default choice performs no resolution; local/cloud/both submit the exact selected action; resolved indicators disappear. |

## Issue found and repaired

The local-deletion branch in `sync::decide` treated an already-confirmed local tombstone as a new deletion when the remote was explicitly restored. This unnecessarily recreated a conflict on the original device. The condition now requires `local != baseline` before treating local deletion as conflicting with a concurrent remote edit. A failing unit regression and the failing real two-device lifecycle reproduced this before the repair; both passed afterward. Unsynchronized local deletion versus independent remote edits still conflicts.

The existing full session integration fixture was adjusted to the new purge contract: pending purge is rejected, and purge succeeds after configuration deletion is acknowledged even if separate session cleanup is waiting for a lease.

## Verification evidence

- Ordinary Rust suite: 229 passed, 33 opt-in ignored. Selected opt-in tests below were run explicitly.
- Frontend suite: 173 passed in 41 files. Sync-specific screens/lifecycle subset: 27 passed.
- Go/PostgreSQL suite: `go test ./... -count=1` passed; `go vet ./...` passed.
- `desktop_two_device_config_sync_lifecycle`: final run passed in 13.53 seconds, including service outage/retry.
- `desktop_real_server_crypto_merge_recovery_and_revocation`: passed in 243.71 seconds, using two-device approval.
- `installed_browsers_preserve_partitioned_cookie_localstorage_and_indexeddb`: all three browsers passed in 193.28 seconds.
- Production TypeScript/Vite build and embedded-frontend local desktop build passed. Diff whitespace checks passed.
- Independent read-only code review found no semantic issue in the restoration correction.

Detailed logs and fixture metadata are under `.qa/mcp/`, including `sync-config-e2e-final.json`, `sync-session-e2e.json`, `sync-all-browsers-storage.log`, `sync-overall-rust-final.log`, `sync-overall-frontend.log`, `sync-server-all.log`, and `sync-server-vet.log`.

Latest local repair executable: `D:\Source\Repos\claudedone\src-tauri\target\debug\nodecloak.exe`; SHA-256 `9504B93ECD6CBC5F678CC539FDD7BE42B486FAC5B149BDEA851CB6919F575DF8`. It has not replaced the user's running executable, been committed/pushed, or been publicly released. These Windows/synthetic-session results are not a claim of macOS native acceptance or successful sign-in to every third-party website.
