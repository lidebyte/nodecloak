package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"image"
	"image/png"
	"net/http"
	"net/http/httptest"
	"testing"
)

type adminTestClient struct {
	h      *harness
	cookie *http.Cookie
	csrf   string
	id     string
}

func (c *adminTestClient) req(method, path string, v any, csrf, origin string) (int, map[string]any) {
	b, _ := json.Marshal(v)
	r := httptest.NewRequest(method, path, bytes.NewReader(b))
	r.RemoteAddr = "127.0.0.1:1234"
	r.Header.Set("Content-Type", "application/json")
	if c.cookie != nil {
		r.AddCookie(c.cookie)
	}
	if csrf != "" {
		r.Header.Set("X-CSRF-Token", csrf)
	}
	if origin != "" {
		r.Header.Set("Origin", origin)
	}
	w := httptest.NewRecorder()
	c.h.s.Handler().ServeHTTP(w, r)
	var out map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &out)
	if path == "/v1/admin/login" && w.Code == 200 {
		c.cookie = w.Result().Cookies()[0]
		c.csrf = out["csrf"].(string)
		c.id = out["user"].(map[string]any)["id"].(string)
	}
	return w.Code, out
}
func newAdminClient(t *testing.T, h *harness, email string) *adminTestClient {
	t.Helper()
	h.login(email, deviceA)
	if _, e := h.db.Exec(context.Background(), "UPDATE users SET role='admin' WHERE email=$1", email); e != nil {
		t.Fatal(e)
	}
	c := &adminTestClient{h: h}
	if n, o := c.req("POST", "/v1/admin/email/request", map[string]any{"email": email}, "", ""); n != 202 {
		t.Fatal(n, o)
	}
	if n, o := c.req("POST", "/v1/admin/login", map[string]any{"email": email, "emailCode": h.mail.codes[email]}, "", ""); n != 200 {
		t.Fatal(n, o)
	}
	return c
}
func TestAdminAuthorizationCSRFAndSuspension(t *testing.T) {
	h := setup(t)
	desktop := h.login("user@example.com", deviceB)
	c := &adminTestClient{h: h}
	if n, _ := h.req("GET", "/v1/admin/users", desktop["accessToken"].(string), nil); n != 401 {
		t.Fatal(n)
	}
	delete(h.mail.codes, "user@example.com")
	if n, _ := c.req("POST", "/v1/admin/email/request", map[string]any{"email": "user@example.com"}, "", ""); n != 202 {
		t.Fatal(n)
	}
	if _, sent := h.mail.codes["user@example.com"]; sent {
		t.Fatal("ordinary user received administrator code")
	}
	c = newAdminClient(t, h, "admin@example.com")
	if !c.cookie.HttpOnly || c.cookie.SameSite != http.SameSiteStrictMode || c.cookie.MaxAge != 28800 {
		t.Fatal("cookie flags", c.cookie)
	}
	for _, test := range []struct{ csrf, origin string }{{"", ""}, {"wrong", ""}, {c.csrf, "https://evil.example"}} {
		if n, _ := c.req("PUT", "/v1/admin/system", map[string]any{"registrationEnabled": true, "supportURL": "https://nodecloak.com", "announcement": ""}, test.csrf, test.origin); n != 403 {
			t.Fatal("mutation allowed", n)
		}
	}
	if n, o := c.req("GET", "/v1/admin/session", nil, "", "http://127.0.0.1:8088"); n != 200 || o["csrf"] != c.csrf {
		t.Fatal(n, o)
	}
	userID := desktop["account"].(map[string]any)["id"].(string)
	if n, o := c.req("PATCH", "/v1/admin/users/"+userID, map[string]any{"disabled": true}, c.csrf, ""); n != 200 {
		t.Fatal(n, o)
	}
	if n, _ := h.req("GET", "/v1/account", desktop["accessToken"].(string), nil); n != 401 {
		t.Fatal("disabled access", n)
	}
	if n, _ := h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": desktop["refreshToken"]}); n != 401 {
		t.Fatal("disabled refresh", n)
	}
	if n, _ := h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "user@example.com"}); n != 202 {
		t.Fatal(n)
	}
	if n, _ := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "user@example.com", "code": h.mail.codes["user@example.com"], "device": securityDevice(deviceB)}); n != 401 {
		t.Fatal("disabled login", n)
	}
	if n, _ := c.req("PATCH", "/v1/admin/users/"+c.id, map[string]any{"disabled": true}, c.csrf, ""); n != 409 {
		t.Fatal("self disable", n)
	}
	if _, e := h.db.Exec(context.Background(), "UPDATE users SET role='user' WHERE id=$1", c.id); e != nil {
		t.Fatal(e)
	}
	if n, _ := c.req("GET", "/v1/admin/users", nil, "", ""); n != 401 {
		t.Fatal("role not checked", n)
	}
}
func TestAdminCatalogDraftPublicationAndIcons(t *testing.T) {
	h := setup(t)
	c := newAdminClient(t, h, "catalog-admin@example.com")
	n, out := c.req("GET", "/v1/admin/catalog", nil, "", "")
	if n != 200 {
		t.Fatal(n, out)
	}
	rev := int64(out["revision"].(float64))
	groups := []catalogGroup{{ID: "tools", Name: "工具", NameEn: "Tools", Icon: "ai", Enabled: true, Sites: []catalogSite{{ID: "chatgpt", Name: "ChatGPT", URL: "https://chatgpt.com/", Icon: "chatgpt", Enabled: true}, {ID: "hidden", Name: "Hidden", URL: "https://example.org", Icon: "globe", Enabled: false}}}}
	if n, o := c.req("PUT", "/v1/admin/catalog", map[string]any{"revision": rev, "groups": groups}, c.csrf, ""); n != 200 {
		t.Fatal(n, o)
	}
	if n, _ := c.req("PUT", "/v1/admin/catalog", map[string]any{"revision": rev, "groups": groups}, c.csrf, ""); n != 409 {
		t.Fatal("stale save", n)
	}
	if n, _ := c.req("POST", "/v1/admin/catalog/publish", map[string]any{"revision": rev}, c.csrf, ""); n != 409 {
		t.Fatal("stale publish", n)
	}
	if n, o := c.req("POST", "/v1/admin/catalog/publish", map[string]any{"revision": rev + 1}, c.csrf, ""); n != 200 {
		t.Fatal(n, o)
	}
	n, out = h.req("GET", "/v1/catalog", "", nil)
	if n != 200 {
		t.Fatal(n, out)
	}
	publicGroups := out["groups"].([]any)
	if len(publicGroups) != 1 || len(publicGroups[0].(map[string]any)["sites"].([]any)) != 1 {
		t.Fatal(out)
	}
	for _, u := range []string{"http://example.com", "https://127.1", "https://0x7f.0.0.1", "https://2130706433", "https://localhost", "https://127.0.0.1", "https://[::1]", "https://example.local", "https://user:pass@example.com", "https://example.com:444"} {
		if safeDestination(u) {
			t.Fatal("unsafe URL", u)
		}
	}
	if n, _ := c.req("POST", "/v1/admin/icons", map[string]any{"data": "data:image/svg+xml;base64," + base64.StdEncoding.EncodeToString([]byte("<svg/>"))}, c.csrf, ""); n != 400 {
		t.Fatal("svg", n)
	}
	var img bytes.Buffer
	_ = png.Encode(&img, image.NewRGBA(image.Rect(0, 0, 3, 4)))
	n, out = c.req("POST", "/v1/admin/icons", map[string]any{"data": "data:image/png;base64," + base64.StdEncoding.EncodeToString(img.Bytes())}, c.csrf, "")
	if n != 201 {
		t.Fatal(n, out)
	}
	r := httptest.NewRequest("GET", out["icon"].(string), nil)
	r.Header.Set("Origin", "https://client.example")
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, r)
	if w.Code != 200 || w.Header().Get("Access-Control-Allow-Origin") != "*" {
		t.Fatal(w.Code, w.Header())
	}
	cfg, e := png.DecodeConfig(bytes.NewReader(w.Body.Bytes()))
	if e != nil || cfg.Width != 128 || cfg.Height != 128 {
		t.Fatal(cfg, e)
	}
}
func TestAdminReadinessAndRegistration(t *testing.T) {
	h := setup(t)
	c := newAdminClient(t, h, "settings-admin@example.com")
	for _, p := range []string{"summary", "users", "devices", "cloud", "audit", "system"} {
		if n, o := c.req("GET", "/v1/admin/"+p, nil, "", ""); n != 200 {
			t.Fatal(p, n, o)
		}
	}
	if n, o := c.req("PUT", "/v1/admin/system", map[string]any{"registrationEnabled": false, "supportURL": "https://t.me/NodeCloak"}, c.csrf, ""); n != 200 {
		t.Fatal(n, o)
	}
	if n, o := c.req("POST", "/v1/admin/announcements", map[string]any{"title": "Notice", "body": "Hello", "published": true}, c.csrf, ""); n != 201 {
		t.Fatal(n, o)
	}
	if n, _ := h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "new@example.com"}); n != 202 {
		t.Fatal(n)
	}
	if n, o := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "new@example.com", "code": h.mail.codes["new@example.com"], "device": securityDevice(deviceB)}); n != 403 || o["error"] != "registration_disabled" {
		t.Fatal(n, o)
	}
	n, out := h.req("GET", "/v1/catalog", "", nil)
	if n != 200 || out["announcement"] != "Hello" {
		t.Fatal(n, out)
	}
	if n, _ := c.req("POST", "/v1/admin/users/"+c.id+"/sessions/revoke", map[string]any{}, c.csrf, ""); n != 200 {
		t.Fatal(n)
	}
	if n, _ := c.req("GET", "/v1/admin/session", nil, "", ""); n != 401 {
		t.Fatal("revocation", n)
	}
}
func TestAdminTOTPAndBootstrap(t *testing.T) {
	h := setup(t)
	ctx := context.Background()
	user := h.login("bootstrap@example.com", deviceA)
	id := user["account"].(map[string]any)["id"].(string)
	if e := BootstrapAdmins(ctx, h.db, []string{"bootstrap@example.com", "absent@example.com"}); e != nil {
		t.Fatal(e)
	}
	var role string
	_ = h.db.QueryRow(ctx, "SELECT role FROM users WHERE id=$1", id).Scan(&role)
	if role != "admin" {
		t.Fatal(role)
	}
	_, _ = h.db.Exec(ctx, "UPDATE users SET role='user' WHERE id=$1", id)
	if e := BootstrapAdmins(ctx, h.db, []string{"bootstrap@example.com"}); e != nil {
		t.Fatal(e)
	}
	_ = h.db.QueryRow(ctx, "SELECT role FROM users WHERE id=$1", id).Scan(&role)
	if role != "user" {
		t.Fatal("bootstrap repromoted", role)
	}
	_, _ = h.db.Exec(ctx, "UPDATE users SET role='admin' WHERE id=$1", id)
	secret, e := h.s.sealTOTP(id, "JBSWY3DPEHPK3PXP")
	if e != nil {
		t.Fatal(e)
	}
	codes, hashes := h.s.backupCodes(id)
	_, e = h.db.Exec(ctx, "UPDATE users SET totp_secret=$2,backup_hashes=$3,password_hash=$4 WHERE id=$1", id, secret, hashes, hashPassword("SecureAdminPassword1!"))
	if e != nil {
		t.Fatal(e)
	}
	c := &adminTestClient{h: h}
	if n, _ := c.req("POST", "/v1/admin/login", map[string]any{"email": "bootstrap@example.com", "password": "SecureAdminPassword1!"}, "", ""); n != 401 {
		t.Fatal(n)
	}
	if n, o := c.req("POST", "/v1/admin/login", map[string]any{"email": "bootstrap@example.com", "password": "SecureAdminPassword1!", "factor": codes[0]}, "", ""); n != 200 {
		t.Fatal(n, o)
	}
	if n, _ := c.req("POST", "/v1/admin/login", map[string]any{"email": "bootstrap@example.com", "password": "SecureAdminPassword1!", "factor": codes[0]}, "", ""); n != 401 {
		t.Fatal("backup reused", n)
	}
	if n, _ := c.req("POST", "/v1/admin/login", map[string]any{"email": "bootstrap@example.com", "password": "SecureAdminPassword1!", "emailCode": "123456"}, "", ""); n != 401 {
		t.Fatal("two primaries", n)
	}
}
func securityDevice(id string) map[string]any {
	return map[string]any{"id": id, "name": "Test device", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(id[0])}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(id[0]) + 1}, 32))}
}
