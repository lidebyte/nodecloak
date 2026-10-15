# Remote MCP and Sync Implementation Plan

> For agentic workers: execute task-by-task with test-first verification and independent review. Root owns integration; no release or deployment.

**Goal:** Complete the previously proposed third phase: scoped encrypted sync and authorized remote MCP device routing.

**Architecture:** Streamable HTTP/OAuth gateway in Go + PostgreSQL; outbound desktop relay reusing native broker grants/jobs; sync previews and scoped execution reuse the existing encryption and leases.

**Tech Stack:** Go net/http + pgx, PostgreSQL, Rust/Tauri/reqwest, React/TypeScript.

**Spec:** docs/specs/remote-mcp.md

**Completion:** The four implementation tasks below are complete. Validation and environment limits are recorded in `docs/specs/remote-mcp-validation.md`; deployment instructions are in `docs/specs/remote-mcp-deployment.md`. Production deployment and macOS runtime validation are outside this local implementation run.

## Global constraints

Chrome/Edge/Firefox; existing Windows/macOS implementation; existing VI controls; keep local Codex confirmation preference; no public browser debugging listener; no credential/session export; no automatic replay; no production deployment or version bump.

## Task 1: OAuth gateway and durable jobs

Files: create `server/internal/api/mcp_{oauth,gateway,relay}.go`, `mcp.sql`, `mcp_test.go`; modify `server.go`, cleanup and deployment docs.

- [x] Write tests against existing HTTP harness for discovery, DCR exact redirects, S256 PKCE, audience, one-time code, refresh rotation/reuse and device consent.
- [x] Run `go test ./internal/api -run MCP -count=1`; confirm missing routes fail before implementation.
- [x] Implement migration version 8 and OAuth endpoints; requests approved only by authenticated approved device.
- [x] Write gateway/session/job tests: repeat same session/request with same arguments returns same job; different arguments rejected; claim once; stale dispatched work never requeues; cancellation and ownership enforced.
- [x] Implement HTTP initialization/list/call and outbound relay poll/update; enforce per-route limits, encrypted arguments/results, expiry and consent revocation.
- [x] Run PostgreSQL integration tests and `go vet ./...`.

Interfaces: `/mcp`, `/.well-known/oauth-protected-resource/mcp`, `/.well-known/oauth-authorization-server`, `/mcp/oauth/{register,authorize,pending,token,revoke}`, authenticated `/v1/mcp/{requests,approve,relay,routes}`. Device route binds dedicated local grant; relay wire jobs carry immutable IDs/tool/arguments/deadline and cancellation requests.

## Task 2: Scoped native synchronization

Files: create `src-tauri/src/mcp/sync.rs`; modify catalog, operations, broker and `cloud/{sync,session_sync}.rs`.

- [x] Add failing schema/permission tests for `sync_status`, `sync_preview`, `sync_run`, `sync_resolve`; reject unselected profiles and stale preview IDs.
- [x] Implement metadata-only previews with client/account/version fingerprint; preserve existing cipher/session guards.
- [x] Refactor existing full-sync entrypoints to delegate to optional selected-profile filter. Scoped calls skip auto-attach and cloud auto-import; retain full desktop sync behavior.
- [x] Add cancellation checks between profile operations and before conflict resolution; destructive resolutions always require approval.
- [x] Run native sync/MCP tests and existing session regression checks.

Interfaces: sync tools take `profile_ids` (1..100); mutations require `preview_id`; resolve adds `profile_id`, `kind`, `choice`. Preview returns actions/conflicts and opaque state fingerprint.

## Task 3: Desktop relay and grants

Files: create `src-tauri/src/mcp/remote.rs`; modify `mcp/{mod,broker,policy}.rs` and cloud transport error mapping.

- [x] Test remote route/account/grant binding and durable duplicate receipts before implementing dispatch.
- [x] Extract shared broker invocation after local authentication; remote invokes through dedicated approved local grant only.
- [x] Add desktop actions to inspect pairing code, consent with selected scopes, and revoke routes; outbound polling runs only for explicit routes and signed-in matching account.
- [x] Persist delivery receipt before submission; update results/cancellation; never retry browser work after ambiguous dispatch.
- [x] Verify disabling MCP, revocation and account switch stop remote execution without affecting local grants.

## Task 4: VI UI and integration verification

Files: create `src/McpRemote.tsx` and tests; modify `McpWorkspace.tsx`, `mcp.ts`, `mcp.css`, English translations and `docs/mcp.md`.

- [x] Add failing UI tests for remote tab, pairing consent, scoped sync permission, connection status, revoke, and per-client confirmation settings.
- [x] Implement existing brand cards, compact controls and inline status; consent uses a modal with exact target/scope and data-transfer disclosure.
- [x] Run frontend tests/build, Go suite/vet, Rust suite; real gateway/device integration with synthetic profiles and screenshot/interaction on all three browsers.
- [x] Independently review auth/replay and sync boundaries, address findings, update validation evidence, reopen desktop development version.
