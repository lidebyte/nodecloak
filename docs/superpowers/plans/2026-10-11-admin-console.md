# Administration Console Implementation Plan

> **For agentic workers:** Use subagent-driven implementation for the independent API, console and desktop catalog units, followed by root integration and review. User has requested design and completion; execute without an additional plan approval.

**Goal:** Complete `/admin/` and make desktop website shortcuts consume its published groups and branded site icons.

**Architecture:** Extend the existing Go API and PostgreSQL migrations, with separate cookie sessions and server-side administrator checks. Build a standalone React console and embed its output in the server binary. Native desktop requests the published catalog and validates launch destinations.

**Tech Stack:** Go, PostgreSQL, React, TypeScript, Vite, Lucide, existing VI tokens, Rust/Tauri.

**Spec:** `docs/specs/admin-console.md`

## Global Constraints

- First administrator: `jamesupereal@gmail.com`, configured bootstrap of an existing user.
- Existing server credentials are preserved, never committed or exposed to browser.
- Browser website data remains end-to-end encrypted; admin storage screens show metadata only.
- Independent units share the exact HTTP shapes in the spec.
- Integration tests use isolated PostgreSQL, never the production database.
- Existing uncommitted product changes remain intact. No commit/push requested.

### Task 1: Go API and administrator security

Files: `server/internal/api/admin*.go`, `admin.sql`, existing server migration/auth/main.

- [x] Add revisioned catalog and admin sessions/audit/settings migrations with existing-account bootstrap.
- [x] Implement cookie auth, CSRF, primary/factor validation, role/suspension, session invalidation.
- [x] Implement all spec endpoints, catalog validation and safe normalized icons.
- [x] Add meaningful authorization, CSRF, CAS/publication, role and suspension tests; run against isolated PostgreSQL.

### Task 2: Browser console

Files: `admin/index.html`, `src/admin/*`, `vite.admin.config.ts`, embedded assets build directory.

- [x] Implement login/session handling and VI responsive sidebar/page shell.
- [x] Implement real dashboard, catalog editor with icon upload and ordering, draft/publish controls.
- [x] Implement searchable/paginated users/devices/cloud/audit, user actions and confirm dialogs.
- [x] Implement server readiness and editable system settings, error/empty/loading states.
- [x] Build and exercise API integration through a local isolated server/browser.

### Task 3: Desktop catalog and brand icons

Files: site shortcut components, `src/browser-sites.json`, `src-tauri/src/browser_sites.rs`, native bridge launch integration, safe builtin icon assets.

- [x] Render each site's own first-party brand icon and configured groups.
- [x] Fetch published catalog, support offline fallback, remove hardcoded grouping limits.
- [x] Validate native launch against current published catalog and reject arbitrary/insecure destinations.
- [x] Verify default brands and runtime group additions via tests/build.

### Task 4: Integration, review and deployment

- [x] Reconcile interfaces and build standalone console before compiling the embedded Go binary.
- [x] Run frontend/Rust/Go checks appropriate to changes, validate real admin denial and CRUD/publish on an isolated local API.
- [x] Deploy binary and `/admin/` Nginx routing while preserving SMTP/Google/env/token pepper, bootstrap only named existing account.
- [x] Verify public assets, unauthorized API denial, catalog and service readiness; record evidence and test limits.

