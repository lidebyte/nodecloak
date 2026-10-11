ALTER TABLE users ADD COLUMN IF NOT EXISTS role text NOT NULL DEFAULT 'user' CHECK(role IN ('user','admin'));
ALTER TABLE users ADD COLUMN IF NOT EXISTS disabled boolean NOT NULL DEFAULT false;
CREATE TABLE IF NOT EXISTS admin_sessions(token_hash text PRIMARY KEY,account_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,csrf text NOT NULL,expires_at timestamptz NOT NULL,created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS admin_email_codes(email text PRIMARY KEY,code_hash text NOT NULL,expires_at timestamptz NOT NULL,attempts integer NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS admin_audit(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,actor_id uuid REFERENCES users(id) ON DELETE SET NULL,actor_email text NOT NULL,event text NOT NULL,target_id text NOT NULL DEFAULT '',detail jsonb NOT NULL DEFAULT '{}',created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS admin_catalog(singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),revision bigint NOT NULL DEFAULT 1,groups jsonb NOT NULL DEFAULT '[]',published_revision bigint NOT NULL DEFAULT 1,published_groups jsonb NOT NULL DEFAULT '[]');
INSERT INTO admin_catalog(singleton) VALUES(true) ON CONFLICT DO NOTHING;
CREATE TABLE IF NOT EXISTS admin_icons(name text PRIMARY KEY,data bytea NOT NULL,created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE IF NOT EXISTS admin_settings(singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),registration_enabled boolean NOT NULL DEFAULT true,support_url text NOT NULL DEFAULT 'https://t.me/nodecloak_official',announcement text NOT NULL DEFAULT '');
INSERT INTO admin_settings(singleton) VALUES(true) ON CONFLICT DO NOTHING;

CREATE TABLE IF NOT EXISTS admin_bootstrap(email text PRIMARY KEY,applied_at timestamptz NOT NULL DEFAULT now());
