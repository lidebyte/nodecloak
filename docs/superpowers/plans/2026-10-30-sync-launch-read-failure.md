# Synchronization must not prevent opening an owned local profile

**Goal:** Preserve manual launch when pre-launch snapshot staging fails, without publishing incomplete data or bypassing cloud conflicts, share restrictions or path guards.

**Evidence:** 3.2.5 feedback contains `session_capture_failed: 无法读取浏览器副本`. The exact string originates in snapshot directory enumeration before browser launch. The old message discards I/O kind/code. A disposable Windows directory held with an exclusive handle reproduces read_dir failure with OS error 32.

**Scope:** Local code and isolated verification only; no changes to customer profiles, live cloud objects or production deployment.

- [x] Add failing regressions for real locked-directory staging, owned manual launch preserving baselines/pending data, strict explicit sync/received shares, bounded transient retry and sanitized I/O diagnostics.
- [x] Avoid copying only known regenerable Chromium metrics/GPU caches at their supported profile locations; preserve same-named site/extension data and Firefox folders.
- [x] Preserve staging I/O operation, storage category and OS error code without paths, URLs or login contents; allow only the existing owner/manual local-only path on these capture failures.
- [x] Keep incomplete captures out of cloud publication and retain diagnostics/paused synchronization state; make synchronization guidance applicable to local errors too.
- [x] Run targeted and full native tests, frontend checks and real disposable browser roundtrips; read-only review the final safety boundaries.

## Verification (Windows, 2026-10-30)

- Red: directory staging, narrow cache exclusions and transient retry regressions failed against the old implementation. Real `prepare_launch` with an exclusively held disposable directory returned the same generic error as the report.
- Green: targeted staging tests and five launch tests passed. The locked-directory launch regression exercises Chrome, Edge and Firefox at both staging and file-stamp collection, retaining baseline/checkpoint revision 5 and pending revision 7. Explicit sync and received shares still reject the failed capture.
- Full native suite: 317 passed, 44 ignored with four test threads. The first unrestricted-parallel run had 316 passed and one failure in an unchanged device-authorization test before capture; that test passed alone, then passed in the complete bounded run. The initial concurrency-related failure cause remains unconfirmed; authorization checks were not modified.
- Real installed Chrome, Edge and Firefox: synthetic large IndexedDB restore/export and existing-profile sentinels passed (one ignored integration test, 113.21 seconds). No customer profile was used.
- Frontend: 216 tests in 48 files passed; production build passed with the existing bundle-size warning.
- Desktop debug build passed. Read-only safety review found no blockers. `git diff --check` passed.
- Private logs: `.qa/sync-launch-20261030/`. The customer's exact OS error and directory remain unknown because 3.2.5 discarded those details. OS error 32 is a local reproduction, not an attribution of the customer's underlying filesystem condition.

## Delivery

Changes are on `codex/sync-launch-read-failure`; application version remains 3.2.5. No package publication, cloud-object changes or production deployment is part of this fix verification.
