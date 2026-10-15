# Local MCP validation — 2026-10-15

Delivery: phases 1 and 2, 31 local stdio tools, profile management and page automation in Chrome, Edge and Firefox. UI follows the existing Ink/Paper palette and Phosphor accents. The desktop owns browser processes, approvals and authorization; the stdio adapter uses the same executable.

## Executed checks

| Check | Result |
| --- | --- |
| `npm test -- --reporter=dot` | 33 files, 138 tests passed |
| `npm run build` | TypeScript and Vite production build passed; existing bundle-size advisory remains |
| `node --test scripts/mcp-page.checks.mjs scripts/fingerprint.checks.mjs` | 17 tests passed |
| `cargo test --manifest-path src-tauri/Cargo.toml --locked` | 146 passed, 16 integration tests skipped by default |
| Native `native_mcp_page_operations_all_browsers` | Passed with actual installed Chrome, Edge and Firefox |
| Native `native_fingerprints_are_stable_across_restart_and_distinct_between_profiles` | Passed for all three browsers |
| Desktop + stdio integration | Actual OS keyring, generated configuration, rmcp initialization, 31 tool schemas and authenticated calls exercised |
| Windows release executable | GUI-subsystem executable exercised through redirected stdio |
| VI inspection | Native desktop screenshots reviewed in dark and light themes; profile picker and approval controls exercised |

Installed browser versions: Chrome 155.0.8059.40, Edge 155.0.4283.45, Firefox 158.0. Tests used temporary profiles, a loopback fixture website and a separate desktop validation data root. Real user profile data and production account credentials were not used.

## Browser behavior exercised

New tab and navigation; visible form input and select results; native pointer clicks; native file input selection; downloaded file presence in the authorized directory; PNG screenshots; same-origin and cross-origin iframe snapshots, clicks and navigation; unchanged root page after frame navigation; JavaScript dialog handling; stale snapshot rejection after replacement/navigation; continued browser control after ordinary tool errors.

Fingerprint regression verifies first document, frame and new-tab observations, repeat stability, restart stability and independence between profiles.

## End-to-end authorization and lifecycle

- Selected-profile scope enforcement and denial for an unselected profile.
- Create/update/clone, fresh clone fingerprint, desktop-confirmed regeneration, cancellation and trash approval.
- Forced controlled launch of a synthetic legacy profile while preserving saved preferences.
- Task-owned tab cleanup while another task's tab remains open.
- Screenshot conversion to an MCP image content block, including asynchronously retrieved results.
- Cross-client job lookup denial, revocation on the next call, document-bound approval rejection after navigation.
- Masked approval values and sanitized persistent records without form values, page text, images, URL queries or profile notes.
- Approval expiration releases the active-job quota. Startup cancellation/revocation rechecks after launch and rolls back only a browser newly started by that job.

Protocol tests cover malformed/oversized frames, exact-origin rules, strict bounded tool arguments, omitted proxy credential/share references, handshake role/nonce/argument binding, direction-separated encrypted messages, wrong key/connection rejection and malformed AEAD nonces. Requests and replies use HKDF-SHA256 plus ChaCha20-Poly1305; the original keyring token never crosses TCP. Live stdio smoke tests exercise the encrypted channel as part of normal calls.

## Reproduction

Run the normal checks above, then opt into the installed-browser tests:

```powershell
$env:MOZ_HEADLESS='1'
cargo test --manifest-path src-tauri/Cargo.toml --lib native_mcp_page_operations_all_browsers --locked -- --ignored --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib native_fingerprints_are_stable_across_restart_and_distinct_between_profiles --locked -- --ignored --nocapture --test-threads=1
```

The local validation harness and screenshots are in the ignored `.qa/mcp` directory. The checked-in workflow runs the page-reference test alongside existing frontend/native checks on the Windows and both macOS targets.

## Platform boundary

Native runtime evidence in this record is Windows only. macOS uses the shared CDP/BiDi and keychain implementation, but macOS build and real-browser validation remain a release gate. This task does not change the release version or publish artifacts. Remote MCP gateways and cloud-sync tools are outside phases 1 and 2.
