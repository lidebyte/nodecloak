# Administration console delivery evidence

2.0.0 release update: current backend release is `f7b6e43ea961`, schema version 5. At the user's request, all 21 active production announcements were archived with audit records; the public list now contains zero notices. The historical publication counts below describe earlier checks. See [release evidence](../releases/2.0.0.md).

Delivered 2026-10-11 at https://nodecloak.com/admin/ with the named existing administrator jamesupereal@gmail.com. Current backend release `5606616fd378`, migration version 5. Local desktop development process rebuilt and restarted against port 1421.

## Announcement pagination and Markdown update

The independent announcement page now requests server pages of ten records, displays totals and page controls, and returns to a valid page when the final entry on the last page is archived. New/edit dialogs include a Markdown toolbar and Edit/Split/Preview modes. Source is preserved on save; the client and admin preview use the same Markdown renderer. Banner summaries remove Markdown formatting. Raw HTML, executable links, credentials in URLs and local/private address URLs are rejected.

Fresh validation: all 89 frontend tests across 22 files passed, including editor mode/selection/length checks and pagination/save/archive regression tests. Full Go tests and go vet passed against isolated PostgreSQL database `nodecloak_announcement_tests`; the pagination integration test exercised 10/10/1 entries, source preservation and page normalization. Six Rust catalog tests passed. Admin and desktop production builds and Linux service compilation passed. Read-only browser fixtures verified actual admin UI pagination at 10/10/1, live Markdown preview, and App headings/lists/tables/code/link rendering. Browser fixture writes were disabled; saves were verified by component/API tests. Screenshots `.qa/announcement-markdown-editor.png` and `.qa/announcement-markdown-app.png` show unpublished fixture content.

Production release `5606616fd378` is active, schema remains version 5, existing credentials are preserved, public health is ready and the served admin JavaScript hash matches the built asset. All 21 existing published announcements remain available; no Markdown fixture notice was published to production.

## Announcement list update

Announcement management is a dedicated sidebar page with create/edit/draft/publish/withdraw/remove controls. System settings no longer contains announcements. Public history excludes drafts, withdrawn and removed entries. Publishing a draft sets its publication time; edits preserve its place and increment its revision. Archive and edit require CAS revisions, CSRF and current administrator authorization. Migration 5 preserves the prior single announcement once. A maximum of 100 active records bounds the public payload, and the native catalog limit is 2 MiB.

Client defaults to the latest publication; closing persists its ID/revision locally, leaving View more available. New or revised notices reappear. History renders Markdown newest first with dates, keyboard focus and Escape close. All 77 frontend tests, 6 Rust catalog tests, Go PostgreSQL tests and go vet passed. Browser checks on the isolated fixture verified draft exclusion, publishing, persistent close after reload and showing a subsequent publication. Screenshots `.qa/announcement-latest-banner.png` and `.qa/announcement-history.png` use test data. Production migration exposed its preserved announcement through public HTTPS; no test announcement was published to production.

## Checks

- `npm run build:admin` and `npm run build` passed. Desktop build retains its existing bundle-size advisory.
- `npm test`: 18 files, 74 tests passed.
- Rust `cargo test ... browser_sites`: 5 tests passed. Existing website launch tests also passed during implementation.
- Go `go test ./... -count=1` against isolated PostgreSQL and `go vet ./...` passed.
- Real browser: local password sign-in; group creation; site creation inside the group; draft save retaining public revision 1; explicit publish changing public revision to 2 and adding the third group; user search and display-name save; announcement save reaching public catalog.
- Production browser: centered VI login renders, both password and email-code forms display. Production authenticated sign-in is left for the account owner; the mailbox credentials and configured provider values were preserved.
- Production HTTP: health 200, admin HTML 200, module script carrying same-origin Origin 200, anonymous user administration 401, catalog 2 groups / 11 sites, all default icon routes valid SVG.
- Release binary hash matches running process, service active, named administrator role verified, environment permissions remain 0600.

## Boundaries

The console supports catalog CRUD/order/enable/upload/draft/publish; user role/status/name management and login-session invalidation; searchable devices and cloud metadata; audit detail; registration control, support link and announcement; and nonsecret service readiness. Server-managed OAuth/SMTP credentials are not editable in the browser. Administrator cloud views cannot read browser configuration or website sessions. Cryptographic device revocation continues through the user's native workflow.

Muse remains a provisional M wordmark because the existing muse.ai destination could not reliably establish the intended brand. Administrators can replace its icon and address. Icon sources and license notices are in `server/internal/api/adminassets/icons/README.md`.

Fixture-only screenshots: `.qa/admin-overview.png`; production login screenshot: `.qa/admin-production-login.png`. Test accounts, groups and announcements are confined to the isolated local database, not production.
