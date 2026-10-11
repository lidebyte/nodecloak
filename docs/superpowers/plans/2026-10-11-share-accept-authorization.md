# Share acceptance authorization implementation plan

> **For agentic workers:** Implement the native and UI units in parallel with subagents after agreeing this contract; the primary agent handles the service and integration. Follow test-first development and independent review.

**Goal:** Send an encrypted invitation before the recipient authorizes a device, then guide authorization directly during acceptance without visiting personal cloud sync.

**Architecture:** Sharing uses logged-in, non-revoked device identities independently from the personal cloud vault. Senders HPKE-wrap snapshots to the recipient's existing pending or approved devices. Acceptance explicitly authorizes the addressed current device for this grant; the service never globally approves a personal-cloud device. No server-side plaintext/key escrow and no sender-online dependency.

**Contract:**

- `/v1/shares/recipient` returns non-revoked recipient devices even when no vault exists. Account name/email must still match. At least one logged-in recipient device is needed to address ciphertext; registration already records a device key. A newly added/unaddressed device requires a new invitation.
- Create uses authenticated non-revoked sender identity, with no personal cloud vault prerequisite; envelope account/device/public-key identity remains checked transactionally.
- Migration 7 adds nullable `authorized_at timestamptz` to `profile_share_envelopes`.
- Received list entries add optional `deviceAccess: 'ready' | 'authorizationRequired' | 'unavailable'`. Ready means this exact matching device envelope has share authorization or legacy current-epoch personal-cloud approval. Unavailable means no addressed current non-revoked device/key matches.
- `POST /v1/shares/{id}/accept` accepts optional `{authorizeDevice:true}`. Pending/unapproved devices require explicit consent; authenticated approval is stored only on that grant's current-device envelope. Grant transition and authorization are atomic; revoked/expired/left/declined grants never reactivate. Existing approved-device acceptance remains backward compatible with empty/null bodies.
- `sharing_action` accept is `{type:'accept',id,authorizeDevice?:boolean}`; absent defaults false. Native creation, acceptance and runtime authorization no longer require the personal vault root. All server grant/account/device/envelope checks and native copy/export/sync restrictions remain.
- Recipient clicks “接受分享”; authorization-required opens an in-place VI dialog displaying this device and explaining share-only access, with “授权并接受”. Cancellation does not accept or authorize. Missing addressed devices show an actionable new-invitation message. Never open the personal sync panel for sharing.

**Files:** Service `sharing.go`, `server.go`, `sharing_test.go`; native `cloud/sharing.rs` and client error allowlist; UI `ProfileSharing.tsx`, `profile-sharing.ts`, translations and tests. Preserve earlier Google, theme and sharing edits; no commit/tag/installer release.

- [x] Service regression tests demonstrate failure, then implement device-scoped consent and list metadata.
- [x] Native tests demonstrate no-vault create/accept/authorize and exact-device decryption; preserve forced expiry/revocation closure.
- [x] UI tests cover logged-in uninitialized accounts, in-place accept consent, cancel, successful import and unaddressed devices; follow VI and Chinese/English.
- [x] Run complete suites and isolated real native/service/browser checks, independent review, backup/deploy migration 7 and readiness; update requirements/verification.

Verified: frontend 126 tests, native 113 tests plus explicitly executed 44.34-second real integration, complete Go suite, browser in-place consent/cancel/retry and light/dark/English. Review identified the zero-active-device error mapping omission; it was covered with failing regressions and fixed before completion. Migration 7 is deployed with database/config backup and healthy public readiness. Details are in `docs/specs/profile-sharing-verification.md`.
