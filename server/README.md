# Account and synchronization service

Go + PostgreSQL. The database stores opaque encrypted objects, device envelopes, account metadata and hashed credentials. Encryption and website data export happen in the native desktop client.

The current production host runs this service with systemd and its existing PostgreSQL installation; see [production deployment](deploy/README.md). Docker Compose below is an alternative deployment method.

The management console is served at `/admin/`. Run `npm run build:admin` from the repository root before building Go after console changes. Set `ADMIN_EMAILS` to the email of an existing account to bootstrap its administrator role once. Register the account first; no account or password is created by bootstrap. Administrators sign in with their existing password or an emailed code, and their two-factor code when enabled. Proxy `/admin/` as well as `/v1/` using the supplied Nginx snippet. See [console requirements](../docs/specs/admin-console.md) and [verification results](../docs/specs/admin-console-validation.md).

Copy `.env.example` to `.env` on the server, use a random database password and generate `TOKEN_PEPPER` with `openssl rand -hex 32`. Never commit `.env`. Configure a Google **web application** OAuth client, with `PUBLIC_URL/v1/auth/google/callback` as its authorized callback. Configure authenticated SMTP with mandatory TLS. Unconfigured providers return 503; no test login route is shipped.

Run `docker compose up -d --build`. Put an HTTPS reverse proxy in front of the loopback API port. PostgreSQL has no public port. Back up PostgreSQL with encryption and retain `TOKEN_PEPPER` separately; losing it invalidates all sessions and OTPs. Never log request bodies, Authorization headers or OAuth callback query strings. Use a dedicated host/server URL and disable reverse proxy query logging for OAuth.

Development and integration tests:

```powershell
$env:TEST_DATABASE_URL='postgres://nodecloak:local-test-only@127.0.0.1:56543/nodecloak_test?sslmode=disable'
go test ./... -count=1
go vet ./...
```

Integration tests **truncate the configured test database**. Use an isolated database; never point TEST_DATABASE_URL at production. Production `.env` and desktop crypto keys are not needed for tests. Mail delivery is injected only by tests, not replaced by a public API returning verification codes.

Device recovery verifies a one-time Ed25519 challenge signature derived from the random recovery secret. Device registration also binds a random authenticator, preventing a stolen account login from claiming an existing approved device ID. Object writes require the approved device, current account key epoch and CAS revision; browser-session writes additionally require a live fenced lease. Tombstones require explicit restoration and are retained to prevent stale offline resurrection.

## Deployment configuration

Set `PUBLIC_URL` to the real external HTTPS origin. Google client ID/secret and all `SMTP_*` settings are read by Go from its environment; none are bundled into the desktop client. SMTP uses mandatory STARTTLS on port 587 or implicit TLS on port 465. Keep the environment file readable only by the deployment owner (`chmod 600 .env`). `TOKEN_PEPPER` must contain at least 32 random bytes encoded as hex (64 hex characters) and must remain stable across restarts.

With a host Nginx proxy and Docker Compose, set `TRUSTED_PROXIES` to the exact bridge gateway address seen by the API, with `/32` for IPv4 or `/128` for IPv6. It is deployment-specific; inspect the Docker network before configuring it. An empty setting ignores forwarding headers. Do not use `0.0.0.0/0`. The API parses a trusted forwarding chain from right to left, so clients cannot replace their rate-limit address with a forged header.

Example Nginx locations inside a TLS server block for the configured host:

```nginx
location = /v1/auth/google/callback {
    access_log off;
    proxy_hide_header Content-Security-Policy;
    proxy_hide_header Referrer-Policy;
    proxy_hide_header X-Content-Type-Options;
    add_header Content-Security-Policy $upstream_http_content_security_policy always;
    add_header Referrer-Policy no-referrer always;
    add_header X-Content-Type-Options nosniff always;
    add_header Permissions-Policy "camera=(), microphone=(), geolocation=()" always;
    proxy_pass http://127.0.0.1:8088;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Forwarded-Proto https;
}
location / {
    client_max_body_size 256m;
    proxy_read_timeout 45s;
    proxy_pass http://127.0.0.1:8088;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Forwarded-Proto https;
}
```

Set `NODECLOAK_API_URL` **before compiling Rust/Tauri** to that same HTTPS origin. The default is `https://nodecloak.com`; the production account service uses the `/v1/` routes alongside the website. Development builds additionally allow HTTP only on `127.0.0.1`/`localhost`. The desktop transport refuses redirects and never falls back to another service URL.

Check `GET /healthz` for database readiness. Email request and Google start return 503 until their provider settings exist. Google requires a verified email claim; associating an existing email account requires an explicit signed-in link operation. Account login alone cannot decrypt cloud objects.

## Data and recovery operations

## Remote MCP gateway

The Go service now includes `/mcp` (Streamable HTTP JSON responses), OAuth PKCE authorization/registration/token/revocation endpoints under `/mcp/oauth/`, and protected-resource/authorization-server discovery. Migration 8 creates the MCP tables on startup. Desktop devices receive work through authenticated outbound `/v1/mcp/relay`; desktop debugging ports are never exposed.

Use the full [Nginx snippet](deploy/nginx-account.conf) and [remote MCP deployment guide](../docs/specs/remote-mcp-deployment.md). The new paths must bypass the static website fallback. Preserve the API's nonce CSP for the consent page and disable OAuth request-URL logging. Keep `PUBLIC_URL`, desktop `NODECLOAK_API_URL` and the OAuth resource origin consistent. No additional environment secret is required: payload encryption uses a separate derived key from the existing stable `TOKEN_PEPPER`.

Gateway payloads expire after fifteen minutes; the main process calls cleanup every minute to erase expired ciphertext independently of traffic. Backup retention is configured separately. Restore the same pepper with database backups; changing it requires an explicit re-encryption migration.

Only explicit grants on signed-in approved devices can execute requests. OAuth grants bind the client, account, device and local grant. Logout revokes that device's routes, scrubs queued payloads and requests cancellation. Refresh token reuse revokes the affected route. Dispatched work is never delivered again, including after device restart.

## Account password and 2FA

Passwords use salted Argon2id (64 MiB, three iterations, two lanes). Hashing endpoints admit at most two concurrent requests and reject excess work; account/IP limits also apply. Password and email-code login require TOTP or a single-use recovery code when 2FA is enabled. Verified Google OAuth intentionally skips application 2FA. Changing account security requires the current password and the enabled second factor; setting the first password requires provider/email authentication within 15 minutes. Refreshing a token never renews that authentication timestamp. Security changes revoke other sessions and pending login challenges.

TOTP enrollment expires after ten minutes and requires a valid code before activation; codes reject reused time steps. Login challenges expire after five minutes and five failed attempts. The ten recovery codes are returned once and stored only as keyed hashes. TOTP seeds are encrypted with AES-GCM using a domain-separated key derived from TOKEN_PEPPER, with account identity authenticated as associated data. Preserve TOKEN_PEPPER with protected backups: changing or losing it also makes enrolled TOTP seeds unreadable. Rotate it only with an explicit re-encryption migration. These server-owned TOTP secrets are distinct from the client-owned encryption keys for browser data.

The service stores only the latest object revision, bounded to 1,000 objects and 256 MiB per account. Deletion replaces website snapshots with encrypted empty snapshots and retains configuration tombstones. Database backups can still contain older ciphertext; define a bounded backup retention policy at deployment. Restoring an old database snapshot can invalidate sessions and trigger client rollback detection. Do not delete device proofs or reset object revisions to make a client accept a restored database.

Revoking an approved device is an atomic transaction: rotate the account root, rewrite latest ciphertext, wrap the new root for the remaining authenticated device roster, fence leases and revoke that device's sessions. The user must save the new recovery key. Other approved devices compare the new vault code or use the new recovery key before using that root. Recovering an account's email access does not recover encryption keys; losing every authorized device and the recovery key makes existing encrypted objects unrecoverable.

Server backups, root encryption keys and login credentials are separate. Restoring the database cannot restore a user's native keyring. The desktop checks previously confirmed object revisions even after local profile removal/recreation. Encrypted local snapshot caches retain only the baseline, pending conflict and recreation checkpoint revisions after successful synchronization.
# Session/configuration consistency

Desktop session writes include `configRevision`, the revision of their confirmed profile configuration. The service checks it together with the configuration's deletion state while holding the same transaction lock used by configuration writes. A change returns `409 config_conflict`; clients must synchronize configuration and resolve its conflicts before retrying the session write. Session CAS, epoch and lease generation checks still apply. Requests from older clients without `configRevision` retain compatibility, but cannot publish a live session for a deleted configuration or delete a session for a live configuration.
