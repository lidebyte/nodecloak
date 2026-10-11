# Profile sharing verification — 2026-10-11

Release follow-up: the completed feature is now committed and published as 2.1.0 from source `9f0ae7a`, with verified Windows x64 and both macOS packages. Sharing service schema 7 is deployed. See [2.1.0 release evidence](../releases/2.1.0.md). The pre-release version and uncommitted status below are historical.

Internal engineering record. Requirements are in `profile-sharing.md`; UI follows `docs/branding.md` and the existing VI reference. Package version remains 2.0.1. Earlier Google icon/loading and homepage theme changes are preserved.

## Checks

| Layer | Result | Evidence |
| --- | --- | --- |
| Frontend | 27 files / 126 tests passed | `npm test`; includes in-place share authorization regressions |
| Production frontend | TypeScript and Vite build passed | `npm run build`; existing bundle-size advisory remains |
| Native unit suite | 113 passed, 10 integration tests ignored by default | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |
| Native sharing tests | 5 passed; real sharing integration explicitly executed separately | `cargo test --manifest-path src-tauri/Cargo.toml --lib cloud::sharing` |
| Native compile | Latest library check passed without warnings | `cargo check --manifest-path src-tauri/Cargo.toml --lib` |
| Service | Complete Go suite passed against isolated PostgreSQL | `go test ./... -count=1`; API suite 10.516 s after consent changes |
| Cross-layer integration | Actual Go/PostgreSQL, Windows credential store and Firefox passed | `desktop_real_sharing_encryption_import_guards_and_revocation`, 44.34 s including no-vault senders/recipients |
| Browser UI | Real React components with native command fixture passed | `.qa/verify-sharing-ui.py`; light/dark/English, no page errors |
| Review | Original four findings plus wrapped-key cleanup fixed; final review found no blockers | Independent native/UI/service follow-up review |
| Whitespace | Passed | `git diff --check` |

Disposable PostgreSQL is bound only to localhost port 56544, database `nodecloak_sharing_test`. Desktop integration uses real email-code login through a test-only mail sink, not a production authentication bypass. Destructive service fixtures were run only after the desktop harness finished. Temporary Firefox profiles contain synthetic cookies/localStorage, and temporary processes and OS credentials were cleaned by the integration test.

## Behaviors exercised

- Username/email account matching; pending grants cannot authorize or download payloads; device approval and wrapped-key addressing; account separation; owner disable/revocation; lifecycle races and terminal non-reactivation.
- Configuration-only and opt-in login snapshots, HPKE / authenticated symmetric encryption, actual Firefox cookie and localStorage restore, pre-import restriction reservation, retry retaining the same local ID, original owner data preserved.
- Received profile edit/copy/reshare/export and personal-cloud attachment rejected by native guards; missing binding file retains registry restrictions.
- Concurrent expired-token connections refresh safely. A deadline edited to two seconds expires in the real service and native authorization/payload retrieval reject subsequent use.
- Exact-profile forced termination closes the temporary received Firefox while the owner's temporary Firefox continues running. Native watchdog policies separately cover account mismatch, lost authorization lease and local deadlines. Quit/update use the raw registry and close received profiles; startup does not recover their relay before authorization.
- Real UI invitation review precedes upload; login state defaults off; accept adds a local profile; received menus omit copy/edit/reshare; deadline editing, revoke, leave and local cleanup require the appropriate confirmations. Cleanup does not mutate server grant state. Polling does not recursively trigger itself.
- New regression initially failed for all three expiry entry points (list, create/quota sweep, authorization). Final implementation deletes wrapped keys atomically with expiration and ciphertext removal; all cases now pass.

## Deployment and local build

The account service was deployed with a private database/configuration backup and automatic previous-binary rollback on readiness failure. Final Linux binary SHA-256:

`e7386928036eab076caa5f619f6edbadc158ac144499e4e21402326c583bab31`

Database migration 7 is active; configuration hash stayed unchanged. Public `https://nodecloak.com/healthz` returned `{"status":"ok"}` and unauthenticated `GET /v1/shares` returned 401 after deployment. Backup location: `/var/backups/nodecloak/sharing-20261011T151216Z-e7386928036e`.

The existing Tauri development watcher rebuilt and launched the latest native executable from `D:\Source\Repos\useclaude\src-tauri\target\debug\nodecloak.exe` and uses the live Vite frontend on port 1421. The installed desktop app was not replaced. No commit, push, tag or installer release was made for this feature.

## Practical limits

Windows runtime and Firefox cross-layer session restore were exercised. macOS runtime and a new signed installer were not exercised in this work. Existing browser-session portability restrictions still apply; browser password stores are not part of the login snapshot. New devices need a fresh invitation from the sender. NodeCloak revocation stops its own shared profile; it cannot recall downloaded information or invalidate sessions directly at third-party websites.

Ignored local artifacts contain screenshots and detailed native validation logs: `.qa/sharing-inbox-light.png`, `.qa/sharing-invite-light.png`, `.qa/sharing-confirm-dark.png`, `.qa/sharing-inbox-en.png`, `.qa/sharing-native-verification.txt`, `.qa/sharing-deployment.json`.

## Earlier recipient approval follow-up (superseded by acceptance consent)

A real send returned `recipient_device_approval_required`: the recipient account matched, but no device satisfied the existing approved/current-vault-epoch query. This is an authorization prerequisite, not a sent invitation or server outage. Known sharing API errors now display actionable Chinese/English messages. Recipient guidance points to Preferences → Cloud sync → Manage sync for device authorization; the sender can retry afterward. The share form announces this prerequisite before review.

Background metadata errors and foreground action errors have separate state, so successful polling cannot erase a failed invitation's instructions. Four new regression cases first failed, then passed after the fix. Browser `.qa/verify-sharing-approval-ui.py` independently exercised the exact native error, preserved review through a metadata event, no invitation on failure, and successful retry after simulated recipient approval in Chinese/English and light/dark themes, with no page errors. Final focused review found no blockers. This follow-up changes only frontend presentation/state; service/native authorization is unchanged.

## Final acceptance consent flow

The user subsequently requested invitations before device approval and device authorization directly when accepting. Sharing now operates independently of the personal cloud vault. Existing logged-in non-revoked device keys, including pending keys, are addressed with the same HPKE encryption. Acceptance explicitly authorizes only the current matching envelope on that grant; personal device status stays pending and no personal vault is initialized. Legacy current-epoch approved devices remain compatible.

Service regressions first reproduced pending sender/recipient denial, then passed with the new consent flow. They verify no payload before acceptance, explicit boolean consent, no consent leakage to a second grant, no implicit personal-cloud approval, unaddressed device denial, public-key changes and device/account revocation, and terminal non-reactivation. The real native integration repeats config/session sharing for both legacy and no-vault accounts, including sender logout before acceptance, HPKE decryption, restricted import, expiry/revocation and exact-profile browser closure. It passed in 44.34 seconds.

Browser `.qa/verify-sharing-consent-ui.py` drove real React components with native command fixtures in Chinese/English and light/dark themes: no-vault sender creates an invitation; recipient sees the current device in an in-place consent dialog; cancel makes no acceptance call; “Authorize and accept” sends explicit consent and imports the profile; no personal-sync panel is opened. No page errors. Screenshots: `.qa/sharing-consent-zh-light.png`, `-zh-dark.png`, `-en-light.png`, `-en-dark.png`.

Independent review found no authorization blocker and identified the missing `recipient_device_unavailable` error mapping for accounts with no active devices. Native and UI regressions reproduced the omission, then verified friendly sign-in guidance and sanitized error handling. The final source/build checks passed. Detailed native evidence is in `.qa/sharing-native-consent-verification.txt`.
