# Local proxy chain implementation plan

> **For agentic workers:** Use executing-plans task-by-task; verify each deliverable before moving on.

**Goal:** Per-profile optional route: browser → local proxy → profile proxy → website, as confirmed by the user.

**Architecture:** Keep the existing authenticated profile relay and credentials. Add an optional local first-hop endpoint (HTTP/HTTPS/SOCKS5, loopback host, no first-hop authentication) to ProxyConfig. Open the profile proxy TCP connection through that hop; retain end-to-end TLS verification for HTTPS proxies and destination DNS resolution at the proxy. Chrome, Edge, Firefox, proxy testing and region lookup share the same relay.

**Tech Stack:** Rust/Tokio/native TLS, React/TypeScript, existing NodeCloak Toggle and SelectMenu.

**Spec:** This document records the confirmed route and concrete local-first-hop scope.

## Global constraints

- Disabled by default, old profiles keep existing behavior.
- First-hop settings are mode/loopback address/port; profile proxy retains its existing keyring-backed username/password.
- Chain failures return specific safe hop errors, never fall back to direct.
- Saved/running configuration snapshots, copy, tests, encrypted sync/share and MCP schemas preserve the chain. Cloud schema 4 prevents older clients silently dropping a required hop.
- Preserve unrelated pending fixes and real user data; no release or commit requested.

### Task 1: Relay and configuration

- Modify src-tauri/src/proxy.rs; create src-tauri/src/proxy/chain_tests.rs.
- [x] Add failing JSON-config tests: reject chain on direct/system; reject invalid endpoints (LAN/domain first hops added in the follow-up below); legacy missing chain stays absent.
- [x] Add real socket tests: HTTP and SOCKS first-hop to HTTP/SOCKS second-hop, remote-only names, independent second-hop auth, CONNECT and plain HTTP, both hop failures without direct fallback.
- [x] Implement LocalProxy and validated optional preProxy, refactor handshake onto an existing socket and dial second-hop through local tunnel; HTTPS TLS wraps the resulting stream.
- [x] Run cargo test proxy:: and repair all struct initializers without changing existing behavior.

### Task 2: Persistence and public schemas

- Modify src-tauri/src/cloud/model.rs, src-tauri/src/mcp/catalog.rs and operations.rs; extend model/profile tests.
- [x] Test profile save/copy/update round trip, active relay snapshots, credentials unchanged when only first hop changes, cloud schema round trip and rejection of preProxy with legacy schema.
- [x] Export chain in schema 4 and accept 1–4; expose secret-free MCP chain metadata and validate endpoint schema.

### Task 3: VI controls

- Modify src/browser-profiles.ts, src/BrowserProfiles.tsx, src/locales/en.json; create src/ProfileProxyChain.tsx and its UI test.
- [x] Fail frontend validation/UI tests for switch, protocol selection, host/port save and turning off; test proxy test draft uses chain and changes invalidate old results.
- [x] Use existing Toggle, SelectMenu and advanced-card tokens; show route preview. Preserve chain when pasting a second-hop URI; clear it when selecting direct/system.
- [x] Verify targeted Vitest, TypeScript/build, native relay tests and full Rust suite; review final diff using requesting-code-review.

## Verification record

- Red: legacy config ignored preProxy and accepted invalid direct/system chains; real tunnel attempted to resolve the deliberately invalid second-hop host directly. Both tests failed before implementation.
- Red: cloud export used schema1 and omitted preProxy; frontend validation accepted a chain on direct mode. Both failed before schema/validation changes.
- Final Rust suite: 211 passed, 29 opt-in ignored, zero failures.
- Final frontend suite: 155 passed in 37 files. Production TypeScript/Vite build passed (existing chunk-size advisory).
- Real socket matrix passed HTTP/SOCKS5 first/second-hop combinations, plain HTTP, CONNECT bytes, remote DNS, second-hop authentication isolation, first/second hop errors, no direct fallback, and mandatory native TLS handshakes for HTTPS hops.
- Native opt-in chain launch passed installed Chrome, Edge and Firefox with disposable local profiles and two distinct local proxy listeners. No real account/profile/network settings were changed.
- Read-only independent feature review found no actionable correctness/security issue.
- UI inspected in a fresh local demo tab: branded switch, protocol dropdown, address/port, route preview; no draft saved during visual inspection.
- Local first hop is an unauthenticated endpoint (local app or reachable LAN/router proxy); second-hop credentials remain in the OS credential store. This scope is explicit, not a claim that first-hop authentication is implemented.
- Current desktop development binary is running; no version bump, release, commit/push, deployment or hibernation was requested/performed.

## Provider import follow-up

User feedback exposed the URI-only parser rejecting provider exports in host:port:username:password form. The parser now supports this form and host:port, respects the selected protocol for forms without a scheme, retains raw password delimiters/percent signs, and supports bracketed IPv6. URI protocol still takes precedence and its percent-encoded credentials decode once. Input controls, malformed/out-of-range ports, invalid hosts and invalid/overlong credentials return fixed error messages without echoing input. Existing preProxy configuration survives recognizing a new second-hop endpoint. Tests use synthetic credentials only: two new regression tests failed before implementation, then all 18 targeted parser/copy/chain UI tests and the production build passed. No connection to the user-provided proxy was attempted and no real credential was copied into files.


## LAN/router first-hop follow-up

- Allow valid IP/domain first hops for soft-router listeners; retain LocalProxy/preProxy serialized names and existing profiles. The endpoint uses the existing second-hop validation rules. Profile authentication remains exclusive to the second hop.
- Frontend labels now say first-hop proxy and explain local/LAN listeners versus transparent proxy gateways. The feature does not configure router routing or firewall rules.
- Red: new Rust and frontend router cases failed against the old loopback-only checks. Green: 221 ordinary Rust tests passed (30 opt-in ignored), 161 frontend tests passed, production TypeScript/Vite build and git diff whitespace check passed. Independent read-only review reported no actionable findings.
- Verified the user's specified router at the HomeProxy mixed-port default using a bounded SOCKS5 no-auth greeting only. No router login, network configuration, profile changes, second-hop credentials, or external destinations were involved. This verifies the listener, not the complete user's second-hop route.
- Inspected an unsaved demo draft with a LAN address and SOCKS5 port; no profile was saved. Desktop development watcher rebuilt and ran the updated binary.
