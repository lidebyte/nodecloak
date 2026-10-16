# Multi-device synchronization tolerance — validation

## Behavior

Normal optimistic revision races are retried against fresh encrypted configuration, up to three attempts. Independent field edits still use the existing three-way merge. Local merged settings are applied after a confirmed upload; repeated races remain queued instead of becoming an overwrite choice.

Startup catches up peer configuration before preparing login data, including configuration-only synchronization. The browser launch reloads the resulting profile so updated preferences, proxy and startup URL are actually used.

Closed profiles release the current device's exact lease generation after successful synchronization, including unchanged sessions and session tombstones. Lease rows are retained so generations keep advancing. Fresh launches request a new generation; heartbeat renewal retains the current generation. Delayed release from an old launch cannot release the new launch or another device's lease.

Waiting for a closed browser, another writer or another device is projected separately from actionable conflicts. Existing persisted records retain their schema. The UI describes automatic retry and leaves takeover opt-in. Incompatible configuration edits, divergent login snapshots, missing records and deletion choices remain explicit.

## Verification

- Failing native regressions reproduced the original configuration-CAS conflict, blocked startup and retained closed-profile lease. Additional red regressions covered bounded retries/waiting projection, configuration-only startup, fresh-launch fencing and acknowledged session deletion.
- Failing PostgreSQL regressions reproduced missing lease release and fresh-launch acquisition support.
- Final Rust suite: **247 passed**, **34 opt-in ignored**. The ten new HTTP/crypto/status regressions use temporary profiles and unique OS credential namespaces.
- Final frontend suite: **188 passed** in 44 files. Production TypeScript/Vite build passed.
- Go/PostgreSQL: `go test ./... -count=1` and `go vet ./...` passed against a disposable loopback-bound PostgreSQL container.
- Real native-client / Go / PostgreSQL configuration lifecycle passed in **14.94 seconds**: initial/unchanged sync, independent merge, genuine same-field local/cloud/both/skip decisions, offline retry, deletion restore/accept/purge and missing-registry protection.
- Real native-client / Go / PostgreSQL session lifecycle passed in **247.39 seconds**, including immediate device handoff, login snapshot transfer, divergent-state preservation, deletion safeguards, recovery, key rotation and device revocation.
- Synthetic UI rendered using the production components in light/dark themes. Waiting has no manual-conflict heading, takeover starts disabled, and no browser page errors occurred.
- Independent read-only review identified delayed same-device release and session-tombstone release gaps. Both received regression coverage and were repaired; follow-up review found no further actionable issue.
- Local desktop debug build passed. No actual production account or browser profile was changed by these tests.

## Compatibility and rollout

Deploy the additive server lease endpoint and optional `newSession` request field before distributing the new desktop client. Existing clients keep their previous behavior. A new client connected to an older server falls back to the legacy launch request; failed/unsupported release retains its generation for retry rather than creating a conflict. The faster release behavior requires the new server endpoint.

The normal automatic synchronization cycle remains 30 seconds. Browser data is captured only after the profile is closed; temporary waiting clears when a later cycle succeeds. Automatic synchronization remains subject to the user's preference.

These results describe the optimization work before publication. The follow-up [3.1.4 release plan](../superpowers/plans/2026-10-16-release-3.1.4.md) tracks deployment and platform packaging. Installed-client replacement and physical macOS native acceptance were not performed during the isolated optimization tests. Genuine divergent login snapshots are deliberately not combined or silently overwritten.

Detailed logs, screenshots and fixture metadata are retained under `.qa/sync-tolerance/`.
