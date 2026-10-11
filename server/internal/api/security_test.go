package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"image"
	"image/png"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

func TestAccountProfileSurvivesAnotherLogin(t *testing.T) {
	h := setup(t)
	a := h.login("profile@example.com", deviceA)["accessToken"].(string)
	var b bytes.Buffer
	if e := png.Encode(&b, image.NewRGBA(image.Rect(0, 0, 2, 2))); e != nil {
		t.Fatal(e)
	}
	avatar := "data:image/png;base64," + base64.StdEncoding.EncodeToString(b.Bytes())
	status, _ := h.req("PATCH", "/v1/account", a, map[string]any{"name": "My name", "avatar": avatar})
	if status != 200 {
		t.Fatal(status)
	}
	again := h.login("profile@example.com", deviceB)
	profile := again["account"].(map[string]any)
	if profile["name"] != "My name" || profile["avatar"] != avatar {
		t.Fatal("login lost saved account profile")
	}
}

func TestForwardedClientIPRequiresTrustedProxyAndIgnoresSpoofedLeftmost(t *testing.T) {
	s := New(nil, Config{TrustedProxies: []string{"127.0.0.1/32"}}, nil)
	r := httptest.NewRequest("POST", "http://localhost", nil)
	r.RemoteAddr = "203.0.113.7:123"
	r.Header.Set("X-Forwarded-For", "192.0.2.44")
	if s.clientIP(r) != "203.0.113.7" {
		t.Fatal("untrusted client spoofed throttle IP")
	}
	r.RemoteAddr = "127.0.0.1:123"
	r.Header.Set("X-Forwarded-For", "192.0.2.44, 203.0.113.7")
	if s.clientIP(r) != "203.0.113.7" {
		t.Fatal("trusted proxy chain accepted spoofed leftmost IP")
	}
	r.Header.Set("X-Forwarded-For", "garbage")
	if s.clientIP(r) != "127.0.0.1" {
		t.Fatal("invalid forwarded address accepted")
	}
}

func TestConcurrentCASOnlyOneWriterWins(t *testing.T) {
	h := setup(t)
	token := h.login("race@example.com", deviceA)["accessToken"].(string)
	h.enroll(token)
	var wins atomic.Int32
	var conflicts atomic.Int32
	var wg sync.WaitGroup
	for n := 0; n < 8; n++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			s, _ := h.req("PUT", "/v1/objects/"+objectA, token, map[string]any{"kind": "config", "baseRevision": 0, "epoch": 1, "payload": "opaque"})
			if s == 200 {
				wins.Add(1)
			} else if s == 409 {
				conflicts.Add(1)
			}
		}()
	}
	wg.Wait()
	if wins.Load() != 1 || conflicts.Load() != 7 {
		t.Fatalf("lost-update protection: wins=%d conflicts=%d", wins.Load(), conflicts.Load())
	}
}
func TestKnownApprovedDeviceIDCannotBeClaimedWithAccountLoginAlone(t *testing.T) {
	h := setup(t)
	a := h.login("owner@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "owner@example.com"})
	status, _ := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "owner@example.com", "code": h.mail.codes["owner@example.com"], "device": map[string]any{"id": deviceA, "name": "impersonator", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{'1'}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{'X'}, 32))}})
	if status != 403 {
		t.Fatalf("approved identity impersonation %d", status)
	}
}
func TestProviderConfigurationFailsClosedAndOAuthRejectsReplay(t *testing.T) {
	h := setup(t)
	s, _ := h.req("POST", "/v1/auth/google/start", "", map[string]any{"device": map[string]any{"id": deviceA, "name": "A", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{2}, 32))}})
	if s != 503 {
		t.Fatalf("Google configuration bypass %d", s)
	}
	s, _ = h.req("GET", "/v1/auth/google/callback?state=unknown&code=unknown", "", nil)
	if s != 400 {
		t.Fatal("unknown OAuth state accepted")
	}
	h.s.mail = nil
	s, _ = h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "test@example.com"})
	if s != 503 {
		t.Fatal("mail configuration bypass")
	}
}
func TestEmailCodeLocksAfterFiveFailures(t *testing.T) {
	h := setup(t)
	h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": "attempts@example.com"})
	valid := h.mail.codes["attempts@example.com"]
	bad := "000000"
	if bad == valid {
		bad = "111111"
	}
	for n := 0; n < 5; n++ {
		s, _ := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "attempts@example.com", "code": bad})
		if s != 401 {
			t.Fatal(s)
		}
	}
	s, _ := h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": "attempts@example.com", "code": valid})
	if s != 401 {
		t.Fatal("OTP brute-force lockout bypassed")
	}
}
func TestUnsafeDeploymentConfigurationRejected(t *testing.T) {
	for _, url := range []string{"http://example.com", "https://user:pass@example.com", "https://example.com/path", "https://example.com/?key=x"} {
		if (Config{PublicURL: url, TokenPepper: strings.Repeat("a", 64)}).Validate() == nil {
			t.Errorf("unsafe URL accepted %s", url)
		}
	}
	if (Config{PublicURL: "https://api.nodecloak.com", TokenPepper: "GENERATE_WITH_OPENSSL_RAND_HEX_32"}).Validate() == nil {
		t.Error("placeholder accepted as token pepper")
	}
}
func TestCookieDataRejectedFromBrowserOrigin(t *testing.T) {
	h := setup(t)
	r := httptest.NewRequest("POST", "/v1/auth/email/request", strings.NewReader(`{"email":"a@example.com"}`))
	r.Header.Set("Origin", "https://attacker.example")
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, r)
	if w.Code != 403 {
		t.Fatal("web origin accepted")
	}
}
func TestGoogleDoesNotSilentlyLinkPreviouslyRegisteredEmail(t *testing.T) {
	h := setup(t)
	h.login("owner@example.com", deviceA)
	r := httptest.NewRequest("POST", "/v1/auth/google/poll", nil)
	w := httptest.NewRecorder()
	d := deviceInput{ID: deviceB, Name: "B", PublicKey: base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), Secret: base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{2}, 32))}
	h.s.completeLogin(w, r, "owner@example.com", "verified-google-subject", d)
	if w.Code != 409 {
		t.Fatalf("email-based implicit Google link %d", w.Code)
	}
	var sub *string
	_ = h.db.QueryRow(context.Background(), "SELECT google_sub FROM users WHERE email='owner@example.com'").Scan(&sub)
	if sub != nil {
		t.Fatal("unconfirmed provider was linked")
	}
}
