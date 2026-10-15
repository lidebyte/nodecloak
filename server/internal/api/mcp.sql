CREATE TABLE mcp_clients (
 id text PRIMARY KEY, name text NOT NULL, redirects text[] NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE mcp_routes (
 id uuid PRIMARY KEY, account_id uuid NOT NULL, device_id uuid NOT NULL, local_grant_id uuid NOT NULL,
 client_id text NOT NULL REFERENCES mcp_clients(id), scopes text[] NOT NULL, tools jsonb NOT NULL,
 expires_at timestamptz NOT NULL, revoked boolean NOT NULL DEFAULT false, last_seen timestamptz,
 FOREIGN KEY(account_id,device_id) REFERENCES devices(account_id,id) ON DELETE CASCADE
);
CREATE TABLE mcp_authorizations (
 id uuid PRIMARY KEY, client_id text NOT NULL REFERENCES mcp_clients(id), redirect_uri text NOT NULL,
 state text NOT NULL, challenge text NOT NULL, resource text NOT NULL, scopes text[] NOT NULL,
 user_code text UNIQUE NOT NULL, poll_token text UNIQUE NOT NULL, grant_id uuid REFERENCES mcp_routes(id) ON DELETE CASCADE,
 code_hash text UNIQUE, code_sealed text, redeemed boolean NOT NULL DEFAULT false,
 expires_at timestamptz NOT NULL DEFAULT now()+interval '10 minutes', created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE mcp_tokens (
 access_hash text PRIMARY KEY, refresh_hash text UNIQUE NOT NULL, route_id uuid NOT NULL REFERENCES mcp_routes(id) ON DELETE CASCADE,
 access_expires timestamptz NOT NULL DEFAULT now()+interval '15 minutes',
 refresh_expires timestamptz NOT NULL DEFAULT now()+interval '30 days', used boolean NOT NULL DEFAULT false
);
CREATE TABLE mcp_sessions (
 id text PRIMARY KEY, route_id uuid NOT NULL REFERENCES mcp_routes(id) ON DELETE CASCADE,
 expires_at timestamptz NOT NULL DEFAULT now()+interval '8 hours'
);
CREATE TABLE mcp_jobs (
 id uuid PRIMARY KEY, route_id uuid NOT NULL REFERENCES mcp_routes(id) ON DELETE CASCADE,
 session_id text NOT NULL REFERENCES mcp_sessions(id) ON DELETE CASCADE, request_id text NOT NULL,
 tool text NOT NULL, argument_hash text NOT NULL, arguments_sealed text NOT NULL DEFAULT '', result_sealed text NOT NULL DEFAULT '',
 state text NOT NULL CHECK(state IN ('queued','dispatched','running','awaiting_approval','completed','failed','cancelled')),
 error text NOT NULL DEFAULT '', cancel_requested boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT now(), expires_at timestamptz NOT NULL DEFAULT now()+interval '15 minutes',
 dispatched_at timestamptz, updated_at timestamptz NOT NULL DEFAULT now(), UNIQUE(session_id,request_id)
);
CREATE INDEX mcp_jobs_dispatch ON mcp_jobs(route_id,created_at) WHERE state='queued';
CREATE INDEX mcp_jobs_expiry ON mcp_jobs(expires_at);
