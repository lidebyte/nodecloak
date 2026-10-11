CREATE TABLE admin_announcements(
 id uuid PRIMARY KEY,
 title text NOT NULL,
 body text NOT NULL,
 revision bigint NOT NULL DEFAULT 1,
 published boolean NOT NULL DEFAULT false,
 deleted boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now(),
 published_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX admin_announcements_public ON admin_announcements(published_at DESC,id DESC) WHERE published AND NOT deleted;
INSERT INTO admin_announcements(id,title,body,published)
 SELECT '00000000-0000-4000-8000-000000000001'::uuid,'公告',announcement,true
 FROM admin_settings WHERE singleton=true AND length(trim(announcement))>0;
