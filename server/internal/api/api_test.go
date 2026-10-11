package api

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"github.com/jackc/pgx/v5/pgxpool"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

type testMail struct{ codes map[string]string }

func (m *testMail) Send(_ context.Context, email, code string) error {
	m.codes[email] = code
	return nil
}

type harness struct {
	*testing.T
	s    *Server
	mail *testMail
	db   *pgxpool.Pool
}

func setup(t *testing.T) *harness {
	t.Helper()
	dsn := os.Getenv("TEST_DATABASE_URL")
	if dsn == "" {
		t.Skip("TEST_DATABASE_URL required for PostgreSQL integration tests")
	}
	pool, e := pgxpool.New(context.Background(), dsn)
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(pool.Close)
	if e = Migrate(context.Background(), pool); e != nil {
		t.Fatal(e)
	}
	_, e = pool.Exec(context.Background(), "TRUNCATE users,email_codes,auth_limits,oauth_flows,admin_bootstrap,admin_email_codes,admin_announcements CASCADE")
	if e != nil {
		t.Fatal(e)
	}
	if _, e = pool.Exec(context.Background(), "UPDATE admin_settings SET registration_enabled=true,support_url='https://nodecloak.com',announcement=''"); e != nil {
		t.Fatal(e)
	}
	mail := &testMail{codes: map[string]string{}}
	s := New(pool, Config{PublicURL: "http://127.0.0.1:8088", TokenPepper: strings.Repeat("a", 64)}, mail)
	return &harness{t, s, mail, pool}
}
func (h *harness) req(method, path, token string, v any) (int, map[string]any) {
	h.Helper()
	b, _ := json.Marshal(v)
	r := httptest.NewRequest(method, path, bytes.NewReader(b))
	r.RemoteAddr = "127.0.0.1:1234"
	r.Header.Set("Content-Type", "application/json")
	r.Header.Set("X-NodeCloak-Account-Security", "1")
	if token != "" {
		r.Header.Set("Authorization", "Bearer "+token)
	}
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, r)
	var out map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &out)
	return w.Code, out
}
func (h *harness) login(email, device string) map[string]any {
	h.Helper()
	status, _ := h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": email})
	if status != 202 {
		h.Fatalf("request code: %d", status)
	}
	status, out := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": email, "code": h.mail.codes[email], "device": map[string]any{"id": device, "name": "Test device", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(device[0])}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(device[0]) + 1}, 32))}})
	if status != 200 {
		h.Fatalf("login: %d %v", status, out)
	}
	return out
}
func (h *harness) enroll(token string) {
	h.Helper()
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{7}, 32))
	s, b := h.req("POST", "/v1/vault/initialize", token, map[string]any{"epoch": 1, "recoveryEnvelope": "recovery-ciphertext", "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "deviceEnvelope": "device-ciphertext", "recoveryVerifier": base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey))})
	if s != 201 {
		h.Fatalf("initialize: %d %v", s, b)
	}
}
func TestRecoveryRequiresProofAndConsumesChallenge(t *testing.T) {
	h := setup(t)
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	b := h.login("owner@example.com", deviceB)["accessToken"].(string)
	status, out := h.req("POST", "/v1/vault/recover/start", b, map[string]any{})
	if status != 200 {
		t.Fatal(status)
	}
	body := map[string]any{"challenge": out["challenge"], "epoch": 1, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "recovered-ciphertext", "signature": base64.StdEncoding.EncodeToString(make([]byte, 64))}
	status, _ = h.req("POST", "/v1/vault/recover", b, body)
	if status != 401 {
		t.Fatalf("false recovery accepted: %d", status)
	}
	body["signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(ed25519.NewKeyFromSeed(bytes.Repeat([]byte{7}, 32)), []byte(out["message"].(string))))
	status, _ = h.req("POST", "/v1/vault/recover", b, body)
	if status != 200 {
		t.Fatalf("recovery failed %d", status)
	}
	status, _ = h.req("POST", "/v1/vault/recover", b, body)
	if status != 401 {
		t.Fatal("recovery replay accepted")
	}
}

const deviceA = "11111111-1111-4111-8111-111111111111"
const deviceB = "22222222-2222-4222-8222-222222222222"
const objectA = "33333333-3333-4333-8333-333333333333"

func TestEmailCodeReplayAndExpiration(t *testing.T) {
	h := setup(t)
	login := h.login("one@example.com", deviceA)
	if login["accessToken"] == nil {
		t.Fatal("missing token")
	}
	s, _ := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "one@example.com", "code": h.mail.codes["one@example.com"]})
	if s != 401 {
		t.Fatalf("code replay accepted: %d", s)
	}
	h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "expired@example.com"})
	_, _ = h.db.Exec(context.Background(), "UPDATE email_codes SET expires_at=now()-interval '1 second' WHERE email=$1", "expired@example.com")
	s, _ = h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "expired@example.com", "code": h.mail.codes["expired@example.com"]})
	if s != 401 {
		t.Fatalf("expired code accepted: %d", s)
	}
}
func TestPendingDeviceAndAccountIsolation(t *testing.T) {
	h := setup(t)
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	s, _ := h.req("GET", "/v1/objects", a, nil)
	if s != 403 {
		t.Fatalf("pending device read: %d", s)
	}
	h.enroll(a)
	s, _ = h.req("PUT", "/v1/objects/"+objectA, a, map[string]any{"kind": "config", "baseRevision": 0, "epoch": 1, "payload": "opaque-config"})
	if s != 200 {
		t.Fatalf("write: %d", s)
	}
	b := h.login("other@example.com", deviceB)["accessToken"].(string)
	h.enroll(b)
	s, _ = h.req("GET", "/v1/objects/"+objectA, b, nil)
	if s != 404 {
		t.Fatalf("cross-account blob leak: %d", s)
	}
	s, _ = h.req("POST", "/v1/devices/"+deviceA+"/revoke", b, nil)
	if s != 404 {
		t.Fatalf("cross-account revoke: %d", s)
	}
}
func TestCASAndTombstoneRequireExplicitRestore(t *testing.T) {
	h := setup(t)
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	write := func(base int, deleted, restore bool) int {
		s, _ := h.req("PUT", "/v1/objects/"+objectA, a, map[string]any{"kind": "config", "baseRevision": base, "epoch": 1, "deleted": deleted, "restore": restore, "payload": "opaque"})
		return s
	}
	if write(0, false, false) != 200 || write(0, false, false) != 409 || write(1, true, false) != 200 || write(2, false, false) != 409 || write(2, false, true) != 200 {
		t.Fatal("CAS or tombstone semantics violated")
	}
}
func TestDeviceApprovalRevocationAndRefreshReplay(t *testing.T) {
	h := setup(t)
	login := h.login("owner@example.com", deviceA)
	a := login["accessToken"].(string)
	h.enroll(a)
	b := h.login("owner@example.com", deviceB)["accessToken"].(string)
	s, out := h.req("GET", "/v1/devices", a, nil)
	if s != 200 {
		t.Fatal(s)
	}
	var fp string
	for _, d := range out["devices"].([]any) {
		v := d.(map[string]any)
		if v["id"] == deviceB {
			fp = v["fingerprint"].(string)
		}
	}
	s, _ = h.req("POST", "/v1/devices/"+deviceB+"/approve", a, map[string]any{"fingerprint": "wrong", "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "ciphertext", "epoch": 1})
	if s != 409 {
		t.Fatal("mismatched fingerprint approved")
	}
	s, _ = h.req("POST", "/v1/devices/"+deviceB+"/approve", a, map[string]any{"fingerprint": fp, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "ciphertext", "epoch": 1})
	if s != 200 {
		t.Fatal(s)
	}
	s, _ = h.req("GET", "/v1/objects", b, nil)
	if s != 200 {
		t.Fatal("approved device denied")
	}
	s, _ = h.req("POST", "/v1/devices/"+deviceB+"/revoke", a, nil)
	if s != 409 {
		t.Fatal("approved device revocation must rotate encryption keys")
	}
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{8}, 32))
	s, _ = h.req("POST", "/v1/vault/rotate", a, map[string]any{"baseEpoch": 1, "revokeDevice": deviceB, "recoveryEnvelope": "replacement-recovery-envelope", "recoveryVerifier": base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey)), "devices": []any{map[string]any{"id": deviceA, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "replacement-device-envelope"}}, "objects": []any{}})
	if s != 200 {
		t.Fatalf("atomic revocation failed: %d", s)
	}
	s, _ = h.req("GET", "/v1/objects", b, nil)
	if s != 401 {
		t.Fatal("revoked device retained access")
	}
	refresh := login["refreshToken"].(string)
	s, rotated := h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": refresh})
	if s != 200 {
		t.Fatal(s)
	}
	s, _ = h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": refresh})
	if s != 401 {
		t.Fatal("refresh replay")
	}
	s, _ = h.req("GET", "/v1/account", rotated["accessToken"].(string), nil)
	if s != 401 {
		t.Fatal("replayed refresh family not revoked")
	}
}
func TestSessionLeaseFencesOldWriter(t *testing.T) {
	h := setup(t)
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	s, lease := h.req("POST", "/v1/objects/"+objectA+"/lease", a, map[string]any{"takeover": false})
	if s != 200 {
		t.Fatal(s)
	}
	put := map[string]any{"kind": "session", "baseRevision": 0, "epoch": 1, "payload": "session-ciphertext", "generation": lease["generation"]}
	if s, _ = h.req("PUT", "/v1/objects/"+objectA, a, put); s != 200 {
		t.Fatal(s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE leases SET expires_at=$1 WHERE object_id=$2", time.Now().Add(-time.Minute), objectA)
	_, newLease := h.req("POST", "/v1/objects/"+objectA+"/lease", a, map[string]any{"takeover": false})
	if lease["generation"] == newLease["generation"] {
		t.Fatal("expired fence reused")
	}
	put["baseRevision"] = 1
	if s, _ = h.req("PUT", "/v1/objects/"+objectA, a, put); s != 409 {
		t.Fatal("stale fence wrote")
	}
}
func TestSessionWriteFencesConfigDeletedAfterClientCheck(t *testing.T) {
	h := setup(t)
	token := h.login("config-fence@example.com", deviceA)["accessToken"].(string)
	h.enroll(token)
	putConfig := func(base int, deleted bool) {
		if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, map[string]any{"kind": "config", "baseRevision": base, "epoch": 1, "deleted": deleted, "payload": "config"}); status != 200 {
			t.Fatal(status)
		}
	}
	putConfig(0, false)
	_, lease := h.req("POST", "/v1/objects/"+objectA+"/lease", token, map[string]any{"takeover": false})
	putConfig(1, true) // Deletion commits after the client's preflight, before session PUT.
	put := map[string]any{"kind": "session", "baseRevision": 0, "epoch": 1, "generation": lease["generation"], "payload": "preserved-login"}
	if status, body := h.req("PUT", "/v1/objects/"+objectA, token, put); status != 409 || body["error"] != "config_conflict" {
		t.Fatalf("late login upload accepted: %d", status)
	}
	put["deleted"] = true
	put["configRevision"] = 1
	if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, put); status != 409 {
		t.Fatal("stale config revision accepted")
	}
	put["configRevision"] = 2
	if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, put); status != 200 {
		t.Fatalf("current deletion rejected: %d", status)
	}
	if status, body := h.req("GET", "/v1/objects/"+objectA+"?kind=session", token, nil); status != 200 || body["deleted"] != true || body["revision"] != float64(1) {
		t.Fatal("rejected writes changed session")
	}
}
func TestRejectOversizedAndUnknownRequestFields(t *testing.T) {
	h := setup(t)
	s, _ := h.req(http.MethodPost, "/v1/auth/email/request", "", map[string]any{"email": "a@example.com", "admin": true})
	if s != 400 {
		t.Fatal("unknown fields allowed")
	}
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	s, _ = h.req("PUT", "/v1/objects/"+objectA, a, map[string]any{"kind": "config", "baseRevision": 0, "epoch": 1, "payload": strings.Repeat("x", 13<<20)})
	if s != 413 {
		t.Fatalf("unbounded object %d", s)
	}
}
func TestSessionWriteFencesLiveConfigurationRevision(t *testing.T) {
	h := setup(t)
	token := h.login("revision-fence@example.com", deviceA)["accessToken"].(string)
	h.enroll(token)
	for base := 0; base < 2; base++ {
		if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, map[string]any{"kind": "config", "baseRevision": base, "epoch": 1, "payload": "edited-config"}); status != 200 {
			t.Fatal(status)
		}
	}
	_, lease := h.req("POST", "/v1/objects/"+objectA+"/lease", token, map[string]any{"takeover": false})
	put := map[string]any{"kind": "session", "baseRevision": 0, "configRevision": 1, "epoch": 1, "generation": lease["generation"], "payload": "login"}
	if status, body := h.req("PUT", "/v1/objects/"+objectA, token, put); status != 409 || body["error"] != "config_conflict" {
		t.Fatalf("stale config accepted: %d", status)
	}
	put["configRevision"] = 2
	if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, put); status != 200 {
		t.Fatalf("current config rejected: %d", status)
	}
}
