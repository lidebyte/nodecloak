CREATE TABLE IF NOT EXISTS users (
 id uuid PRIMARY KEY, email text UNIQUE NOT NULL, google_sub text UNIQUE,
 display_name text NOT NULL, avatar bytea, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS email_codes (
 email text PRIMARY KEY, code_hash text NOT NULL, expires_at timestamptz NOT NULL,
 attempts int NOT NULL DEFAULT 0, issued_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS auth_limits (
 key text PRIMARY KEY, started_at timestamptz NOT NULL DEFAULT now(), count int NOT NULL
);
CREATE TABLE IF NOT EXISTS devices (
 account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, id uuid NOT NULL,
 name text NOT NULL, public_key text NOT NULL, secret_hash text NOT NULL,
 fingerprint text NOT NULL, status text NOT NULL CHECK(status IN ('pending','approved','revoked')),
 envelope text NOT NULL DEFAULT '', epoch int NOT NULL DEFAULT 0, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(account_id,id)
);
CREATE TABLE IF NOT EXISTS sessions (
 id uuid PRIMARY KEY, account_id uuid NOT NULL, device_id uuid NOT NULL, family uuid NOT NULL,
 access_hash text UNIQUE NOT NULL, refresh_hash text UNIQUE NOT NULL,
 access_expires timestamptz NOT NULL, refresh_expires timestamptz NOT NULL, revoked bool NOT NULL DEFAULT false,
 FOREIGN KEY(account_id,device_id) REFERENCES devices(account_id,id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS sessions_family ON sessions(family);
CREATE TABLE IF NOT EXISTS vaults (
 account_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 epoch int NOT NULL CHECK(epoch>0), recovery_envelope text NOT NULL, recovery_verifier text NOT NULL
);
CREATE TABLE IF NOT EXISTS recovery_challenges (
 account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, device_id uuid NOT NULL,
 challenge_hash text NOT NULL, expires_at timestamptz NOT NULL,
 PRIMARY KEY(account_id,device_id)
);
CREATE TABLE IF NOT EXISTS objects (
 account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, id uuid NOT NULL,
 kind text NOT NULL CHECK(kind IN ('config','session')), revision bigint NOT NULL CHECK(revision>0),
 epoch int NOT NULL CHECK(epoch>0), deleted bool NOT NULL DEFAULT false,
 payload text NOT NULL, updated_at timestamptz NOT NULL DEFAULT now(), writer uuid NOT NULL, generation bigint NOT NULL DEFAULT 0,
 PRIMARY KEY(account_id,id,kind)
);
CREATE TABLE IF NOT EXISTS leases (
 account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, object_id uuid NOT NULL,
 owner uuid NOT NULL, generation bigint NOT NULL, expires_at timestamptz NOT NULL,
 PRIMARY KEY(account_id,object_id)
);
CREATE TABLE IF NOT EXISTS audit (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 device_id uuid, event text NOT NULL, object_id uuid, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS oauth_flows (
 state_hash text PRIMARY KEY, poll_hash text UNIQUE NOT NULL, nonce text NOT NULL, verifier text NOT NULL,
 device jsonb NOT NULL, expires_at timestamptz NOT NULL, result jsonb, consumed bool NOT NULL DEFAULT false,
 callback_used bool NOT NULL DEFAULT false
);
