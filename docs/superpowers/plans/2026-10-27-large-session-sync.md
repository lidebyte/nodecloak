# Larger encrypted login snapshots implementation plan

> Execute inline in the existing workspace, using test-driven-development and verification-before-completion. User authorization is to optimize synchronization; release publication remains a separate request.

**Goal:** Allow complete login snapshots up to 64 MiB, compress before encryption, and retain explicit bounds and compatibility with existing snapshots.

**Architecture:** Keep envelope v1 for plaintext up to 8 MiB. Use envelope v2 for larger plaintext, with a separately authenticated format domain and an encrypted, versioned gzip/raw frame containing its expected decoded length. Keep the existing single-object revision/lease transaction, rather than introducing a second chunk transaction protocol. A 96 MiB wire bound accommodates the worst-case uncompressed 64 MiB frame after Base64, so incompressible data is supported too. New readers support both formats; old readers reject v2 safely. Account quotas remain unchanged.

**Spec:** `docs/specs/large-session-sync.md`

**Tech stack:** Rust, existing XChaCha20Poly1305 envelopes, flate2 gzip with the existing locked dependency, browser storage codec, Go/PostgreSQL opaque object service.

## Constraints

- No partial/truncated snapshot or silent omission of site data.
- Plaintext/decoded output at most 64 MiB; data transport/cache at most 96 MiB; isolated browser messages at most 144 MiB to accommodate Firefox's nested JSON representation.
- Preserve small v1 writes, existing ciphertext reads, identity/revision/epoch authentication, conflict handling and atomic restore.
- Authenticate before decompressing; cap decompressed output, validate exact declared length, CRC, codec and trailing bytes.
- Retain account quotas, use bounded concurrent large object handling, preserve normal small API timeouts and use a longer bounded timeout for data transfers.
- Do not alter user browser data, production keys, service configuration or publish a version during implementation/testing.

## Task 1: Authenticated bounded envelope transport

Files: new `src-tauri/src/cloud/session_limits.rs`, new `src-tauri/src/cloud/session_payload.rs`; modify `crypto.rs`, `mod.rs`, `Cargo.toml` and `Cargo.lock`.

- [x] Add failing tests: >8 MiB compressible and incompressible round trips, v1 compatibility, v2 tampering/domain changes, invalid gzip/truncation/length/oversize/trailing bytes.
- [x] Implement shared limits and gzip/raw frame encoder/decoder. `encode(plain: &[u8]) -> Result<Zeroizing<Vec<u8>>, String>` and `decode(frame: &[u8]) -> Result<Zeroizing<Vec<u8>>, String>` are private native helpers. Frame contains a fixed magic, codec, u64 decoded length and bytes.
- [x] Keep v1 seal for <=8 MiB; choose v2 only for larger input. Reject over 64 MiB before compression. Bind v2 to different AAD for both data and wrapped key. New open accepts v1/v2 and validates before restoring.
- [x] Run crypto/payload tests and confirm a large serialized snapshot round trip.

## Task 2: Storage, network and service alignment

Files: `browser_session.rs`, `session_local.rs`, `session_codec.js`, `session_sync.rs`, `client.rs`, `src/locales/en.json`, `scripts/session-codec.checks.mjs`, `server/internal/api/server.go`, `sharing.go`, `rotation.go`, plus bounded-payload service tests.

- [x] Add failing tests for >8 MiB storage snapshots, exact UTF-8 aggregate limits before mutation, large client responses, service object upload/download/revision conflicts, sharing and rotation accepting supported sizes while enforcing limits/quotas.
- [x] Apply shared byte limits to native snapshot validation, local storage, cache reads and WebSocket message configuration. Keep count/depth validation and network-isolated export/restore.
- [x] Enforce browser codec total bytes on export/import; permit supported single values without weakening record/depth checks.
- [x] Permit bounded 96 MiB object/share envelopes and cached responses, preserve 256 MiB account quotas and existing rotation aggregate bounds. Bound in-flight large data operations and use bounded transfer timeouts; leave authentication/admin/media limits unchanged.
- [x] Add actionable translated oversize/unsupported-format errors; do not suggest deleting cookies or silently trimming data.

## Task 3: Full-path verification and review

- [x] Run frontend tests/build, native unit tests, JavaScript codec tests and isolated Go/PostgreSQL tests.
- [x] Run real Chrome, Edge and Firefox large-storage export/restore in disposable profiles, and an isolated two-device encrypted sync with >8 MiB state. Verify stale revisions/failed captures preserve both sides.
- [x] Request read-only review of capacity boundaries, decompression, compatibility and mutation ordering; resolve blockers.
- [x] Record evidence and report capacity and older-client/server requirements accurately. No release or live user-data mutation is included in this task.

## Verification evidence (2026-10-27)

Private command output is retained under `.qa/large-session/` (ignored by Git).

- Native full suite: 312 passed, 44 explicitly ignored (`rust-final.log`); ignored real-browser and two-device cases were run separately.
- Frontend: 216 tests in 48 files passed (`frontend.log`); production build passed (`frontend-build.log`, existing chunk-size advisory only).
- Browser script checks: 39 passed (`script-final.log`), including large single values and UTF-8 rejection before mutation.
- Go/PostgreSQL full suite passed against the disposable local database (`go-final.log`). Original eight-writer CAS and oversized-config tests remain unchanged and pass.
- Installed Chrome, Edge and Firefox each restored and re-exported the exact >8 MiB IndexedDB fixture while preserving an existing sentinel (`native-browsers.log`).
- Two-device native sync passed: source snapshot compressed and encrypted as v2, receiving device restored exact bytes, cloud revision unchanged by receipt (`interop.log`).
- Deployment nginx template syntax passed in a disposable container (`nginx-check.log`).
- Supplemental boundary cases: exact 64 MiB payload roundtrip / next-byte rejection (`payload-boundary.log`); oversized configuration rejected during rotation with original session epoch/revision/payload intact, followed by successful large-session rotation (`rotation-boundary.log`).
- Final read-only review found no remaining blockers. Large-transfer admission initially affected small CAS writes; known-small writes now bypass those slots, preserving the original concurrency contract. Configuration limits are enforced in both upload and rotation.

No production deployment, live browser-data mutation, version bump or release publication was performed. A compatible service/proxy rollout and updated receiving clients are required before enabling larger snapshots in a release.
