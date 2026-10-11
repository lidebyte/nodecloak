ALTER TABLE users ADD COLUMN IF NOT EXISTS password_hash text NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN IF NOT EXISTS totp_secret text NOT NULL DEFAULT '';
ALTER TABLE users ADD COLUMN IF NOT EXISTS totp_last_counter bigint NOT NULL DEFAULT -1;
ALTER TABLE users ADD COLUMN IF NOT EXISTS backup_hashes text[] NOT NULL DEFAULT '{}';
ALTER TABLE users ADD COLUMN IF NOT EXISTS security_version integer NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS auth_at timestamptz NOT NULL DEFAULT now();
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS auth_method text NOT NULL DEFAULT 'legacy';
CREATE TABLE IF NOT EXISTS login_challenges (
 token_hash text PRIMARY KEY, account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 device jsonb NOT NULL, method text NOT NULL, security_version integer NOT NULL,
 attempts integer NOT NULL DEFAULT 0, expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS totp_setups (
 account_id uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
 session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
 secret text NOT NULL, security_version integer NOT NULL,
 attempts integer NOT NULL DEFAULT 0, expires_at timestamptz NOT NULL
);
