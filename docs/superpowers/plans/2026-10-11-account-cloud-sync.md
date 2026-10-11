# Account and Cloud Synchronization Implementation Plan

**Goal:** Ship account management and encrypted cross-device profile/session synchronization.
**Architecture:** Go HTTP service and PostgreSQL migrations; Rust native key vault and sync engine; React account/settings dialogs. Native client owns plaintext. Provider adapters and database transactions isolate server boundaries.
**Tech Stack:** Go, pgx, PostgreSQL, RustCrypto XChaCha20-Poly1305, HPKE, OS keyring, Tauri, React.
**Spec:** `docs/specs/account-cloud-sync.md`

Execution is inline because implementation is already authorized. Each behavior is introduced with a failing test, implemented, then verified.

- [x] Server foundations: configuration validation, migrations, Docker development deployment, readiness and graceful shutdown. Tests use a real temporary PostgreSQL database.
- [x] Authentication: email OTP, Google authorization code with state/PKCE/nonce, hashed rotating sessions, profile edit/avatar validation, logout. Provider secrets remain server-side.
- [x] Vault/device APIs: first-device enrollment, recovery envelope, pending device requests, approval fingerprint, revocation and audit records. Unauthorized devices cannot read blobs.
- [x] Ciphertext APIs: object manifest, transactional CAS writes, tombstones, bounded retention, device session leases with fenced writes. Cross-account and stale writes fail.
- [x] Native cryptography/vault: account-bound AEAD envelope, random data keys, device HPKE and recovery wrapping; native-only keyring access. Tamper, replay and wrong-key tests.
- [x] Native sync: account-isolated associations/baselines, three-way merge and conflict choices, initial pull selection, auto/manual synchronization, local detach versus cloud deletion.
- [x] Browser sessions: controlled managed browser export/import, cookies/local storage/IndexedDB serialization, closed-browser checks, snapshot leasing and validation. Windows Chrome/Edge/Firefox tested with synthetic state and restart. Earlier probes covered Windows/macOS API transfer; full new native Mac validation remains pending.
- [x] Desktop UI: sidebar account, Google/email flow, avatar/name edit, first-sync choice, recovery save/restore, devices, sync preference and conflicts. Match existing compact menus.
- [x] Verification: Go tests/vet/race, Rust tests, frontend tests/build, local service/desktop integration. Document actual provider setup/deployment and any unresolved limitations.


## Deployment follow-through

- [x] Deploy the Go service to the target server, configure real Google OAuth credentials in the protected server environment and verify public HTTPS readiness.
- [ ] Verify real Google sign-in after registering the callback, and configure/test SMTP delivery. No mail provider credentials have been supplied.
- [ ] Build and test the new native cloud client on macOS, including OS keychain/device approval and website snapshot import. The prior cross-device probes cover browser feasibility only.

Verified implementation results are recorded in `docs/specs/account-cloud-sync-validation.md`.
