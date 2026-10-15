# Account synchronization verification — 2026-10-11

Release update: the implementation is committed and published as 2.0.0 from source `007bbeb`. Windows x64 and both macOS architectures passed release builds and CI. Backend release `f7b6e43ea961` is deployed. This supersedes the historical uncommitted/not-built-on-Mac status below; full interactive native cloud integration on macOS remains separate from the successful build/unit tests. See [2.0.0 release evidence](../releases/2.0.0.md).

Later functional-closure fixes on the same day supersede the historical status below: frontend 68 tests, Rust lib 103 passed/9 ignored, Go/PostgreSQL tests and vet passed. The expanded native/Go/PostgreSQL/Firefox integration passed in 173.63 seconds, including deletion conflicts, keep-both session preservation, and launch during lease-blocked deletion. Configuration revision/deletion checks now fence session writes atomically. Production release `2fdbac9fc3c9` is healthy; the user has exercised real Google login. SMTP is now configured: STARTTLS authentication, process configuration, public email-request acceptance and self-mailbox delivery passed. Interactive email-code redemption and 2FA email login have not been exercised. See [functional review and fixes](account-functional-review-2026-10-11.md) for exact scope and limitations.

The implementation remains uncommitted. The Go account service was deployed on 2026-10-11 and Google OAuth and SMTP credentials were configured server-side; deployment verification is recorded in `server/deploy/README.md`. Real Google sign-in was exercised by the user, and SMTP delivery to the supplied sender's own mailbox was verified.

## Verified

- Frontend: `npm test` — 56 tests across 13 files; `npm run build` — passed. The existing bundle exceeds Vite's 500 KiB chunk warning threshold; this is a warning, not a failed build.
- Session serializer: `npm run test:session-codec` — 4 tests, including cyclic/binary structured-clone values, malformed graphs/prototype pollution and non-exportable keys.
- Rust: `cargo test -- --test-threads=1` — 99 passed, 9 explicitly ignored browser/network/keyring tests. `cargo build` — passed on Windows. Ignored cloud/browser tests listed below were run separately.
- Go + isolated PostgreSQL 17: full `go test ./... -count=1` and `go vet ./...` — passed. Linux Docker `CGO_ENABLED=1 go test -race ./... -count=1` and vet — passed. No production database was used.
- Current native implementation: explicit installed Chrome, Edge and Firefox round-trip test passed. Persistent Secure/HttpOnly cookie fields, LocalStorage and IndexedDB were read back using actual browser APIs and checked after restart, using temporary synthetic profiles.
- Real Go/PostgreSQL service + native OS credential store integration passed with both recovery-secret bootstrap and two-device approval/vault-code verification. The final approval run took 38.44 seconds; recovery run took 40.71 seconds.
- Integration exercises independent configuration merge, unlink/remove/re-subscribe with the same cloud identity, encrypted Firefox login-state migration, concurrent login-state changes with "keep both", encrypted empty session tombstones, another device's lease blocking deletion while a surviving profile still synchronizes, retry clearing the waiting conflict, and atomic device revocation/root rotation.
- Authentic old session ciphertext was refused before browser I/O, preserving the confirmed binding and browser snapshot. The recreation checkpoint survived a disk reload and still refused the replay. Unit coverage checks authentic older ciphertext for both config and session kinds, plus altered content at a confirmed revision.
- Cryptographic/device tests reject different accounts, public keys, device IDs, epochs, modified envelopes and incorrect recovery proposals. Tentative roots are separate from confirmed OS-keyring entries. Cloud responses cannot silently overwrite a confirmed root.
- Server tests cover account/device isolation, OTP expiry/attempts/replay, refresh replay, CAS concurrent writers, lease fencing, approval identity, transactional rotation, avatar/name persistence after login, provider fail-closed behavior and spoofed forwarding chains.
- React render tests verify escaped user names, sidebar login, disabled sync controls before recovery acknowledgment, new-device vault-code verification, and non-destructive waiting UI for blocked deletions.
- In-app preview: sidebar login and Google/email login panel inspected visually; browser preview correctly disables native authentication.
- Docker image built successfully. The non-root, read-only container with all Linux capabilities dropped returned `200 {"status":"ok"}` from `/healthz` against the isolated PostgreSQL database. The smoke container was stopped afterward.
- Read-only code review found and verified fixes for device trust, rollback, profile recreation and deletion retry boundaries. No remaining concrete P1/P2 finding in its last pass.

## Not yet verified or deployed

The user has exercised actual Google login after registering the callback. SMTP delivery was verified using the supplied server credentials, but interactive email-code redemption and 2FA email login remain untested in production. Unconfigured providers return 503 rather than simulating success. The new full Rust/cloud flow has not been built/run on macOS; earlier Windows↔macOS synthetic browser API probes established basic migration feasibility, but are separate from this implementation's full integration.

Third-party websites can reject copied sessions or require reauthentication. The tested cookies were persistent; browser-exit cookies may disappear before export. Standard partitioned persistent storage is now supported; see `partitioned-session-storage-validation.md`. Opaque/nonce partitions, unsupported containers, named storage buckets and non-exportable keys fail explicitly. Snapshot scope and IndexedDB generator limitations are documented in `account-cloud-sync.md`.

## Reproduction

Use a disposable PostgreSQL database: the Go tests truncate their configured tables. The test-only Go `TestDesktopInteropHarness` writes an endpoint and synthetic mailbox code into a unique `DESKTOP_INTEROP_DIR`; the ignored Rust `desktop_real_server_crypto_merge_recovery_and_revocation` reads those files and creates isolated native keyring/profile fixtures. Set `DESKTOP_INTEROP_APPROVAL=1` only for the approval variant. The Go harness exists exclusively in a test binary and does not ship with the service. Run shared-database integrations serially.

Production setup, HTTPS proxy, `TRUSTED_PROXIES`, provider variables, backup retention and compile-time `NODECLOAK_API_URL` are described in `server/README.md`.
