package api

import (
	"context"
	"fmt"
	"testing"
)

func TestAdminAnnouncementsPaginationPreservesMarkdown(t *testing.T) {
	h := setup(t)
	c := newAdminClient(t, h, "announcement-pages@example.com")
	body := "## Update\n\n**Ready**\n\n- One\n- Two"
	for i := 0; i < 23; i++ {
		_, err := h.db.Exec(context.Background(), "INSERT INTO admin_announcements(id,title,body,published,published_at,deleted) VALUES($1,$2,$3,$4,now()-$5::int*interval '1 second',$6)", uuid(), fmt.Sprintf("Notice %02d", i), body, i%2 == 0, i, i >= 21)
		if err != nil {
			t.Fatal(err)
		}
	}
	seen := map[string]bool{}
	for page, expected := range []int{10, 10, 1} {
		n, result := c.req("GET", fmt.Sprintf("/v1/admin/announcements?page=%d&pageSize=10", page+1), nil, "", "")
		if n != 200 || result["total"] != float64(21) || result["page"] != float64(page+1) || result["pageSize"] != float64(10) {
			t.Fatal(n, result)
		}
		items := result["items"].([]any)
		if len(items) != expected {
			t.Fatal("page size", page+1, len(items))
		}
		for _, value := range items {
			item := value.(map[string]any)
			id := item["id"].(string)
			if seen[id] || item["body"] != body {
				t.Fatal("duplicate or changed Markdown", item)
			}
			seen[id] = true
		}
	}
	if len(seen) != 21 {
		t.Fatal("missing entries", len(seen))
	}
	n, result := c.req("GET", "/v1/admin/announcements?page=0&pageSize=999", nil, "", "")
	if n != 200 || result["page"] != float64(1) || result["pageSize"] != float64(20) {
		t.Fatal("pagination bounds", n, result)
	}
}

func TestLegacyAnnouncementMigrationPreservesContent(t *testing.T) {
	h := setup(t)
	ctx := context.Background()
	if _, e := h.db.Exec(ctx, "DROP TABLE admin_announcements; DELETE FROM schema_versions WHERE version=5; UPDATE admin_settings SET announcement='Existing notice'"); e != nil {
		t.Fatal(e)
	}
	if e := Migrate(ctx, h.db); e != nil {
		t.Fatal(e)
	}
	list, e := h.s.announcements(ctx, true)
	if e != nil || len(list) != 1 || list[0].Body != "Existing notice" {
		t.Fatal(list, e)
	}
	if e := Migrate(ctx, h.db); e != nil {
		t.Fatal(e)
	}
	list, e = h.s.announcements(ctx, true)
	if e != nil || len(list) != 1 {
		t.Fatal("duplicate migration", list, e)
	}
}

func TestAdminAnnouncementsPublicationAndRevision(t *testing.T) {
	h := setup(t)
	c := newAdminClient(t, h, "announcements@example.com")
	if n, _ := c.req("POST", "/v1/admin/announcements", map[string]any{"title": "First", "body": "First body", "published": true}, "", ""); n != 403 {
		t.Fatal("CSRF", n)
	}
	n, first := c.req("POST", "/v1/admin/announcements", map[string]any{"title": "First", "body": "First body", "published": true}, c.csrf, "")
	if n != 201 {
		t.Fatal(n, first)
	}
	id := first["id"].(string)
	n, draft := c.req("POST", "/v1/admin/announcements", map[string]any{"title": "Second", "body": "Second body", "published": false}, c.csrf, "")
	if n != 201 {
		t.Fatal(n, draft)
	}
	_, catalog := h.req("GET", "/v1/catalog", "", nil)
	if len(catalog["announcements"].([]any)) != 1 || catalog["announcement"] != "First body" {
		t.Fatal(catalog)
	}
	path := "/v1/admin/announcements/" + draft["id"].(string)
	n, published := c.req("PATCH", path, map[string]any{"title": "Second", "body": "Second body", "published": true, "revision": draft["revision"]}, c.csrf, "")
	if n != 200 {
		t.Fatal(n, published)
	}
	_, catalog = h.req("GET", "/v1/catalog", "", nil)
	if catalog["announcement"] != "Second body" || catalog["announcements"].([]any)[0].(map[string]any)["id"] != draft["id"] {
		t.Fatal(catalog)
	}
	if n, _ = c.req("PATCH", path, map[string]any{"title": "Stale", "body": "Stale body", "published": true, "revision": draft["revision"]}, c.csrf, ""); n != 409 {
		t.Fatal("stale edit", n)
	}
	if n, _ = c.req("DELETE", path, map[string]any{"revision": published["revision"]}, c.csrf, ""); n != 200 {
		t.Fatal("archive", n)
	}
	_, catalog = h.req("GET", "/v1/catalog", "", nil)
	if catalog["announcement"] != "First body" {
		t.Fatal(catalog)
	}
	if n, _ = h.req("GET", "/v1/admin/announcements", "", nil); n != 401 {
		t.Fatal("anonymous admin", n)
	}
	if n, _ = c.req("PATCH", "/v1/admin/announcements/"+id, map[string]any{"title": "", "body": "Body", "published": true, "revision": first["revision"]}, c.csrf, ""); n != 400 {
		t.Fatal("blank title", n)
	}
}
