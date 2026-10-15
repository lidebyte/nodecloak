# Remote MCP validation — 2026-10-15

Scope: phase 3 synchronization and remote device routing. Windows local development build and isolated Go/PostgreSQL gateway. No release version bump, GitHub push or production deployment performed.

## Verified behavior

- Real Go HTTP endpoints, PostgreSQL 16 and the native desktop relay completed OAuth dynamic registration, S256 code exchange, desktop consent and remote MCP initialization.
- Each remote client receives a dedicated local grant bound to its account, approved device and gateway origin. Browser work passes through the existing broker's profile/site/file checks.
- Chrome, Edge and Firefox: temporary profiles created and started; actual input-event text and actual click effects asserted; navigation URL and target content asserted; valid PNG screenshot received through MCP ImageContent; profiles stopped.
- A first Edge click initially reported success without the fixture changing. Activating the selected top-level tab immediately before the trusted click fixed the observed failure. The subsequent full three-browser remote run passed.
- Selected-profile sync changed only the selected bound profile while a second profile stayed pending. Changed local data invalidated its preview. Remote version pins additionally reject revision drift after preview validation.
- A remote delete awaiting approval expired without deleting the profile. Logging out canceled another delivered pending approval and invalidated its remote access token.
- Missing relay result fields remain JSON null; intermediate running/approval state polling remains readable.
- Duplicate request IDs return the same job; changed arguments return conflict. Sequential and concurrent relay polls dispatch a job once. Queued cancellation prevents delivery. Another account cannot claim the route or update its job.
- Wrong audience, unregistered redirect and wrong verifier are rejected. Authorization codes are single use; refresh-token reuse revokes the route.
- Expired payload ciphertext is scrubbed by background cleanup, including terminal jobs. AEAD binds job identity and argument/result direction. Logout revokes device routes.
- Sync deadlines are checked before/after network requests, around authentication retries and between profile operations, independently of relay polling. An expired request is rejected before account/keyring/network work.
- VI UI: dark/light screenshots, 900px minimum desktop window width without horizontal overflow, grant modal and sync permission controls. Active status requires an unexpired grant and a currently active gateway route. UI console errors: none.

## Commands and results

| Check | Result |
| --- | --- |
| `npm test` | 33 files, 141 tests passed |
| `npm run build` | passed; existing bundle-size advisory remains |
| `cargo test --lib` | 152 passed, 16 ignored, no failures |
| `go test ./... -count=1` with isolated `TEST_DATABASE_URL` | passed |
| `go vet ./...` | passed |
| Final expanded `go test ./internal/api -run MCP -count=1` | passed |
| Real gateway/device/browser E2E | all three browsers passed |
| New stdio adapter initialization/catalog/native status | 35 tools, four sync schemas present, native status completed |
| Independent read-only review | no remaining P1/P2 findings after fixes |

The 16 ignored native tests include installed-browser/network integration scenarios; the separate remote E2E exercised the real installed Chrome/Edge/Firefox backends. This does not imply every ignored test was run in this turn.

Local QA artifacts (ignored from Git): `.qa/mcp/remote-e2e-report.json`, `remote-{chrome,edge,firefox}.png`, `remote-ui-{dark,light,narrow,grant}.png`, `remote-ui-report.json`, `phase3-adapter-report.json`. The E2E used a synthetic account, temporary browser profiles, isolated app data and a dedicated disposable PostgreSQL database. Real user browser profiles were not used for mutations.

## Local handoff

Normal desktop development mode restored using the production API origin, real app data and existing Vite server. Temporary WebView diagnostic port removed. Original Codex grant retained its profile list and `confirmInteractions=false`; sync scope remains opt-in from the MCP UI. Codex configuration points at a separate updated adapter copy, so rebuilding the desktop does not lock its executable. Restart the Codex MCP connection to reload the four new tool schemas.

## Remaining environment verification

Production service and Nginx/CDN routing are not deployed or smoke-tested here. Follow `remote-mcp-deployment.md` before enabling online access. macOS source paths share the relay/sync implementation, but no macOS runtime/build was available for this validation. These are deployment/platform verification limits, not evidence of an online release.
