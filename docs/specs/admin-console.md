# NodeCloak administration

Implement a working Go/PostgreSQL-backed web console at `/admin/`, styled with the existing NodeCloak VI. The first administrator is `jamesupereal@gmail.com`; bootstrap only this explicitly configured existing account, never the first registrant. Ordinary users cannot enter the console.

## Functional scope

- Dashboard with real counts and readiness.
- Website groups and sites: add/edit/remove, English label, enabled state, ordering, HTTPS destination and brand icon. Edit a draft and explicitly publish; concurrent draft saves use revision CAS. Desktop consumes the published catalog, with the bundled catalog for offline first launch.
- Users: search and pagination, profile/security flags, enable/disable, admin/user roles, invalidate login sessions. Do not let the last admin disappear or let an admin disable/demote themselves.
- Devices: search and pagination, account, approval state, epoch and active sessions; invalidate device login sessions. Only a user's authorized native device can rotate encryption keys and revoke cryptographic approval, so do not pretend server session invalidation rotates keys.
- Cloud storage: inspect account/object metadata, revision, deletion, bytes and timestamps. No plaintext or ciphertext download or mutation from the admin UI.
- Logs: administrator changes and existing synchronization/security audit events, pagination and filters.
- Announcements: independent sidebar page, server pagination with 10 entries per page, create/edit/draft/publish/withdraw/archive controls. Markdown editor with formatting toolbar and Edit/Split/Preview modes; preview and desktop share the same renderer and VI styles.
- System: SMTP/Google readiness and non-secret configuration, database readiness, registration switch and support link. Secrets remain deployment-owned, never sent to the browser.

## HTTP contract

All JSON. Errors use `{error:string}` and non-2xx status. Lists return `{items:[],total:number,page:number,pageSize:number}`. Dates are RFC3339. Admin requests use an HttpOnly session cookie; mutation header `X-CSRF-Token` must match the session CSRF token and same-origin policy. No tokens in localStorage. Sessions expire after eight hours; role/disabled state checked against the database on every request. Password/email login enforces the user's enabled TOTP or recovery factor. Disabled users also lose desktop authentication/refresh/login access.

- `POST /v1/admin/email/request {email}` -> 202. Rate limited, only existing active administrators receive mail.
- `POST /v1/admin/login {email,password?,emailCode?,factor?}` -> `{user:{id,email,name,role},csrf}`. Exactly one primary credential. Generic rejection for absent/non-admin accounts.
- `GET /v1/admin/session` -> same authenticated session payload.
- `POST /v1/admin/logout {}` -> `{ok:true}`.
- `GET /v1/admin/summary` -> `{users,devices,activeSessions,cloudObjects,cloudBytes,publishedRevision,auditEvents}`.
- `GET /v1/admin/catalog` -> `{revision,groups,publishedRevision}`.
- `PUT /v1/admin/catalog {revision,groups}` -> updated draft. Stale revision returns 409.
- `POST /v1/admin/catalog/publish {revision}` -> draft with publishedRevision. Audit publication.
- `GET /v1/catalog` -> `{revision,groups}` containing only enabled groups/sites from last publication.
- `GET /v1/catalog/icons/{name}` -> local safe builtin SVG or uploaded normalized PNG, no remote icon proxy.
- `POST /v1/admin/icons {data}` -> `{icon}`. PNG/JPEG/WebP input bounded and normalized; SVG upload refused. `data` is an image data URL.
- `GET /v1/admin/users?q=&page=1&pageSize=20` -> items `{id,email,name,role,disabled,createdAt,googleLinked,hasPassword,twoFactorEnabled,devices,cloudBytes}`.
- `PATCH /v1/admin/users/{id} {role?,disabled?,name?}` -> `{ok:true}`.
- `POST /v1/admin/users/{id}/sessions/revoke {}` -> `{ok:true}`.
- `GET /v1/admin/devices?q=&page=1&pageSize=20` -> items `{id,accountId,email,name,status,epoch,createdAt,activeSessions}`.
- `POST /v1/admin/devices/{accountId}/{id}/sessions/revoke {}` -> `{ok:true}`.
- `GET /v1/admin/cloud?q=&page=1&pageSize=20` -> items `{accountId,email,id,kind,revision,epoch,deleted,bytes,updatedAt,writer}`.
- `GET /v1/admin/audit?q=&page=1&pageSize=20` -> items `{id,actorEmail,event,targetId,detail,createdAt}` (include existing user audit).
- `GET /v1/admin/system` -> `{smtpConfigured,googleConfigured,databaseReady,publicURL,registrationEnabled,supportURL}`.
- `PUT /v1/admin/system {registrationEnabled,supportURL}` -> same. Last settings save audited. Registration switch affects new user creation, not existing sign-in.
- `GET /v1/admin/announcements?page=1&pageSize=10` -> `{items:[{id,title,body,revision,published,createdAt,updatedAt,publishedAt}],total,page,pageSize}` sorted newest first. `body` stores Markdown source unchanged; archived entries are excluded.
- `POST /v1/admin/announcements {title,body,published}` -> created record. `PATCH /v1/admin/announcements/{id}` accepts the same fields plus the current revision; stale edits return 409.
- `DELETE /v1/admin/announcements/{id} {revision}` archives the entry. Client history excludes archived/withdrawn/draft entries. The public catalog includes `announcements` and the latest body in the legacy `announcement` field. The client shows the latest, remembers local dismissal by ID/revision, and keeps View more available for history.

Catalog group: `{id,name,nameEn,icon,order,enabled,sites:[]}`. Site: `{id,name,url,icon,order,enabled}`. IDs max 80 `[a-zA-Z0-9_-]`, unique across catalog; group/site counts bounded. Icon is builtin slug (`ai`, `shopping`, `claude`, `chatgpt`, `gemini`, `grok`, `muse`, `amazon`, `ebay`, `etsy`, `walmart`, `shopee`, `lazada`, `globe`) or `/v1/catalog/icons/<hash>.png`. Site destinations require HTTPS, no credentials, no literal/local/private IP, no localhost, no nonstandard port. Server never fetches destination pages. Published icons are first-party assets. Native launch also validates catalog destinations; frontend cannot bypass validation.

Announcement Markdown supports headings, emphasis, lists, block quotes, tables, task lists, fenced code, images and links. Raw HTML is omitted. Only validated public HTTPS image/link URLs are rendered; native announcement links open through a validated external-link command. Banner and table excerpts use plaintext summaries. Client history initially renders ten entries and appends ten as the user scrolls.

## Deliverables and verification

Go migration, admin authorization and endpoints, embedded production console assets, safe catalog/icons, desktop/native consumption, API authorization/CAS/suspension tests against isolated PostgreSQL, console build and real browser CRUD/publish verification, production deployment preserving existing credentials and desktop data. Deployment permission is implied by the existing server feature work; no user ciphertext or production accounts are deleted during validation.
