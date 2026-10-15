# Remote MCP deployment

Status: implementation and isolated local integration validated. This work does not deploy the production gateway or change the release version.

## Prerequisites

- Back up PostgreSQL and the existing protected environment file. Preserve `TOKEN_PEPPER`; it protects TOTP secrets and now also authenticated MCP payload ciphertext.
- Build and deploy the updated Go service through the existing service/Compose workflow. Startup applies migration 8 under the existing migration transaction.
- Use the same external HTTPS origin in `PUBLIC_URL` and the desktop build's `NODECLOAK_API_URL`. Default desktop origin: `https://nodecloak.com`.
- Keep the existing exact trusted-proxy allowlist. The gateway uses the existing request/IP limits.

## Reverse proxy

Apply `server/deploy/nginx-account.conf` within the HTTPS website server block. Validate with `nginx -t` before reload. In addition to existing `/v1/` paths, route these to the Go service:

- `/.well-known/oauth-protected-resource/mcp`
- `/.well-known/oauth-authorization-server`
- `/mcp` and `/mcp/oauth/*`

Preserve Authorization and `Mcp-Session-Id` headers; Nginx forwards them by default. Exclude these paths from CDN caching, bot challenges and static fallback. Do not log authorization query strings, request bodies, bearer headers or relay responses. Preserve the service CSP for the authorization page. Current transport returns JSON to each POST; GET `/mcp` intentionally returns 405, as permitted for servers without a standalone SSE stream.

The desktop polls outbound HTTPS. No new inbound firewall rule or public CDP/BiDi listener is needed. Keep the normal HTTP response timeout above 30 seconds; browser execution is asynchronous and clients poll `jobs_get`.

## Smoke verification after deployment

1. `/healthz` returns healthy PostgreSQL status.
2. Discovery returns the exact HTTPS resource `/mcp` and matching authorization-server issuer.
3. An unauthenticated POST `/mcp` returns 401 with `WWW-Authenticate` pointing at protected-resource metadata.
4. A client completes dynamic registration and S256 authorization using an exact registered redirect and resource audience. Desktop consent names the client and callback and limits profiles, scopes, origins, directories and lifetime.
5. Initialize, list tools, call `profiles_list` and poll its job. Repeat for a temporary profile in Chrome, Edge and Firefox with a local fixture; validate actual fill/click effects, navigation and screenshot content.
6. Reuse a request ID with the same body and confirm the same job; alter its body and confirm rejection. Disconnect the desktop and confirm no replay after reconnect.
7. Revoke the route or log out and confirm token rejection and pending-job cancellation. Never use real form submissions for this check.
8. Observe one-minute cleanup: expired job argument/result ciphertext and authorization-code ciphertext are cleared even without incoming MCP traffic. Define database backup retention separately.

Existing local stdio connections continue to work independently of the cloud gateway. A remote client needs a fresh OAuth consent after its route is revoked. Ambiguous browser dispatch is surfaced as indeterminate and requires checking the actual page before another operation.

## Automated checks

Use a dedicated disposable database: the API integration harness truncates account tables. Run `go test ./... -count=1` with `TEST_DATABASE_URL` pointing only to that database, then `go vet ./...`. Run frontend tests/build and native unit tests. The local evidence is recorded in `remote-mcp-validation.md`; macOS runtime and production reverse-proxy smoke checks remain deployment gates.
