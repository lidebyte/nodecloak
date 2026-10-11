# Profile Sharing Implementation Plan

> **For agentic workers:** Implement the following independent units in parallel with subagents, then integrate and review the completed feature. Steps use checkbox syntax for tracking.

**Goal:** Share a frozen browser profile with a named friend who must accept it, with expiry and revocation enforced by the desktop client.

**Architecture:** PostgreSQL grants hold metadata plus opaque per-device encrypted snapshots. Rust imports snapshots into independently restricted profiles and gates launch and ongoing use. React uses the existing account controller and VI to provide invite, inbox, sent list and profile badges.

**Tech Stack:** Go/PostgreSQL, Rust/Tauri/HPKE/XChaCha20Poly1305, React/TypeScript/Vitest/Playwright.

**Spec:** `docs/specs/profile-sharing.md` contains exact route/type contracts and approved rules.

## Global Constraints

- Preserve uncommitted Google icon and homepage theme fixes.
- Use existing account name and email pair; never expose a directory or session payload before acceptance.
- Configuration default; login state opt-in; deadline begins on send.
- Native enforcement every launch and at most 60 seconds during runtime regardless of UI visibility or sync preferences.
- No share replication into personal cloud storage; no received-profile copy/reshare through native commands.
- Follow existing VI, support light/dark and Chinese/English.
- Do not change the release version or Git tags.

## Tasks

- [x] Service (`server/internal/api/sharing.go`, `sharing.sql`, `sharing_test.go`, route/migration registration in `server.go`): write lifecycle/account-isolation/expiry/race/device-envelope tests; demonstrate failure; implement atomic transitions and server authorization; run against isolated PostgreSQL; build Linux service.
- [x] Native (`cloud/sharing.rs`, profile launch/registry guards, scheduler registration and cloud eligibility): write encryption/eligibility/deadline/binding tests first; implement bridge/actions, retryable encrypted snapshot import, durable restrictions and watchdog; run native tests/check.
- [x] Frontend (`ProfileSharing.tsx`, `profile-sharing.ts`, `sharing.css`, `App.tsx`, `BrowserProfiles.tsx`, profile typing and `locales/en.json`): write interaction/validation/expiry tests first; implement VI invite confirmation/inbox/sent/withdrawal/deadline editing/leave flows; preserve existing pending changes; run frontend checks and build.
- [x] Integration: check exact contract against all layers; run complete frontend/native/service suites, real isolated API lifecycle and browser light/dark/English scenarios; review access checks and failure recovery; deploy backward-compatible service with backup/rollback/readiness, build local dev native app if available; document verification and limitations.

Final evidence: `docs/specs/profile-sharing-verification.md`. Review findings were corrected before the final deployment, including forced closure, missing binding-store enforcement, credential refresh serialization, local cleanup of failed imports and atomic expired wrapped-key deletion.

## Verification commands

- `npm test` and `npm run build`.
- Rust: configured cargo runtime, `cargo test --lib cloud::sharing` then appropriate profile/cloud tests and `cargo check --lib`.
- Go: configured toolchain, `TEST_DATABASE_URL=<isolated database> go test ./...`; Linux `go build ./cmd/...`.
- Browser: native command fixture drives real components, verifies pending cannot launch, acceptance materializes profile, terminal states disable it, deadline editing and leave/revoke dialogs, dark/light/English, no page errors.
- Public service: health, unauthenticated sharing access rejected, migration applied, existing account endpoints preserved. Never run destructive test fixtures against production database.
