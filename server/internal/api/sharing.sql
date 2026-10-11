CREATE TABLE profile_shares (
 id uuid PRIMARY KEY,
 sender_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 recipient_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 source_profile_id text NOT NULL,
 profile_name text NOT NULL,
 browser text NOT NULL,
 sender_name text NOT NULL, sender_email text NOT NULL,
 recipient_name text NOT NULL, recipient_email text NOT NULL,
 includes_session boolean NOT NULL,
 note text NOT NULL DEFAULT '',
 status text NOT NULL CHECK(status IN ('pending','accepted','declined','revoked','left','expired')),
 created_at timestamptz NOT NULL, expires_at timestamptz NOT NULL,
 accepted_at timestamptz,
 payload text NOT NULL,
 writer uuid NOT NULL,
 CHECK(sender_id <> recipient_id)
);
CREATE INDEX profile_shares_sender ON profile_shares(sender_id,created_at DESC);
CREATE INDEX profile_shares_recipient ON profile_shares(recipient_id,created_at DESC);
CREATE INDEX profile_shares_expiration ON profile_shares(expires_at) WHERE status IN ('pending','accepted');
CREATE TABLE profile_share_envelopes (
 share_id uuid NOT NULL REFERENCES profile_shares(id) ON DELETE CASCADE,
 device_id uuid NOT NULL,
 public_key text NOT NULL,
 envelope text NOT NULL,
 PRIMARY KEY(share_id,device_id)
);
