# Clear cloud sync workspace

**Goal:** Replace the sync modal with a sidebar page that explains status and safe next actions.

**Architecture:** Keep the existing encrypted sync, revision checks and conflict resolvers. Separate initial setup from everyday status. Render grouped, config-first conflicts before waiting items and linked profiles. Use explicit inline confirmation for overwrite, takeover and unlink actions; do not modify real user data while testing.

**Stack:** React, TypeScript, existing VI tokens, Vitest and browser preview.

- [x] Add navigation regression coverage for sidebar, settings and targeted error links.
- [x] Add workflow coverage for config/session ordering, waiting, stale confirmation, failed actions and setup preferences.
- [x] Extract SyncWorkspace from AccountWorkspace; retain device authorization and recovery gates.
- [x] Show a status summary, needs-attention section, automatic waiting and linked profiles. Keep profile selection in a separate edit view.
- [x] Replace the choice dropdown with direct actions and consequence descriptions. Cancel safely and keep failed actions visible.
- [x] Match current VI, include keyboard focus, narrow layouts and English copy.
- [x] Run frontend suite, production build and synthetic browser visual checks. No release or live account changes in this task.

## Validation

- Frontend suite: 212 tests across 48 files passed.
- TypeScript and Vite production build passed (existing large-chunk advisory remains).
- Synthetic browser preview: light Chinese, dark English at 800px; no open sync dialog or horizontal overflow.
- Configuration decision reveals the subsequent session decision; editor and first-setup selections checked in the browser.
- Independent code review found stale-preview, waiting-lease and translation issues; all addressed and re-reviewed without blockers.
- No real profile, cloud account data or server deployment modified.
