# Local profile launch and partitioned storage

This document records the 2.1.1 launch fallback. The subsequent partition-aware
session migration implementation is documented in
`partitioned-session-storage-validation.md`; normal supported partitions now
synchronize. The fallback remains for unsupported namespaces, with a regression
fixture using a Firefox container rather than an ordinary site partition.

## Report

A local Firefox profile that had previously been shared failed to launch with
`此副本包含分区网站存储，暂不支持完整迁移` on NodeCloak 2.1.0.

## Cause and fix

The launch path checked received-share authorization correctly, but then personal
automatic session synchronization attempted to export the owner's closed profile.
The exporter deliberately rejects partitioned site storage to avoid incomplete
migration. Its error incorrectly prevented local browser launch as well.

Launch preparation now treats this specific **local export** limitation as local
use only. It retains the export error in sync status, clears the outgoing lease
generation, marks the local session dirty, and leaves session baselines, hashes,
file stamps and pending cloud revisions intact. Explicit synchronization and share
export continue to reject incomplete snapshots. Configuration conflicts, other
export errors and received-share authorization still block as before.

The fallback is inside export reconciliation, before any session upload or restore.
It does not depend on whether the owner has sent a sharing invitation, and it does
not alter or remove partitioned site storage.

## Verification

- Regression reproduced the original error before the fix using a synthetic
  Firefox profile with partitioned storage and an isolated local HTTP service.
- After the fix, launch preparation passed without requesting a lease, publishing
  a snapshot or replacing local session files.
- Pending cloud revisions remained intact; explicit sync, received-profile export
  and unrelated export failures remained errors.
- Existing configuration conflicts remained blocking.
- The received-share import guard also asserts that an unready share cannot launch.
- `cargo test --locked --lib`: 115 passed, 0 failed, 10 intentionally ignored.
- `npm run build`: passed.

Tests use unique temporary profiles and OS credential entries. No real user
cookies, login tokens or profile storage are used as test fixtures.

## Persistent launch errors in the interface

The profile list used one error state for both user actions and background list
refreshes. Successful polling (every six seconds) or native change events cleared
a launch failure before the user could read it.

Operation errors and list-loading errors now have separate state. A recovered list
load clears only its own error. Operation errors remain through polling and native
events, until dismissal or the next explicit action. The banner has a branded close
control with localized accessible text.

The regression failed against the original behavior and passed after the fix. It
covers three polling cycles, both native change events, dismissal, list recovery
and successful launch retry. `npm test`: 128 passed across 27 files;
`npm run build`: passed.
