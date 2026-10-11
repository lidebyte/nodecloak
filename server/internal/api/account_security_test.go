package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestPasswordHashAndTOTPStandardVectors(t *testing.T) {
	hash := hashPassword("test password with enough length")
	if !checkPassword(hash, "test password with enough length") || checkPassword(hash, "incorrect") || checkPassword("malformed", "incorrect") {
		t.Fatal("password verification failed")
	}
	if hash == hashPassword("test password with enough length") {
		t.Fatal("password salts reused")
	}
	// RFC 6238, SHA-1 test vectors with eight digits.
	for _, v := range []struct {
		time int64
		code string
	}{{59, "94287082"}, {1111111109, "07081804"}, {1111111111, "14050471"}, {1234567890, "89005924"}, {2000000000, "69279037"}, {20000000000, "65353130"}} {
		if totpCode([]byte("12345678901234567890"), v.time/30, 8) != v.code {
			t.Fatal("TOTP standard vector mismatch")
		}
	}
}

func TestPasswordTwoFactorAndGooglePolicy(t *testing.T) {
	h := setup(t)
	login := h.login("security@example.com", deviceA)
	access := login["accessToken"].(string)
	password := "test-password-123456"
	status, out := h.req("POST", "/v1/account/password", access, map[string]any{"password": password, "currentPassword": "", "code": ""})
	if status != 200 {
		t.Fatalf("set password: %d %v", status, out)
	}
	status, out = h.req("POST", "/v1/account/2fa/start", access, map[string]any{"currentPassword": password, "code": ""})
	if status != 200 {
		t.Fatalf("start TOTP: %d %v", status, out)
	}
	secret := out["secret"].(string)
	seed := decodeTOTPSecret(secret)
	now := time.Now()
	h.s.now = func() time.Time { return now }
	status, _ = h.req("POST", "/v1/account/2fa/enable", access, map[string]any{"code": "invalid"})
	if status != 401 {
		t.Fatal("TOTP enabled without valid proof")
	}
	status, out = h.req("POST", "/v1/account/2fa/enable", access, map[string]any{"code": totpCode(seed, now.Unix()/30, 6)})
	if status != 200 || len(out["backupCodes"].([]any)) != 10 {
		t.Fatalf("enable TOTP: %d", status)
	}
	backups := out["backupCodes"].([]any)
	device := map[string]any{"id": deviceB, "name": "Password test", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{2}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{3}, 32))}
	body := map[string]any{"email": "security@example.com", "password": password, "device": device}
	status, out = h.req("POST", "/v1/auth/password", "", body)
	if status != 202 || out["twoFactorRequired"] != true || out["accessToken"] != nil {
		t.Fatalf("password bypassed TOTP: %d", status)
	}
	challenge := out["challenge"].(string)
	// Enabling consumed this time step; it must not be accepted again.
	status, _ = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": totpCode(seed, now.Unix()/30, 6)})
	if status != 401 {
		t.Fatal("TOTP replay accepted")
	}
	now = now.Add(30 * time.Second)
	status, out = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": totpCode(seed, now.Unix()/30, 6)})
	if status != 200 || out["accessToken"] == nil {
		t.Fatalf("TOTP login failed: %d %v", status, out)
	}
	status, _ = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": backups[0]})
	if status != 401 {
		t.Fatal("completed challenge reused")
	}
	status, out = h.req("POST", "/v1/auth/password", "", body)
	challenge = out["challenge"].(string)
	status, _ = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": backups[0]})
	if status != 200 {
		t.Fatal("backup code rejected")
	}
	status, out = h.req("POST", "/v1/auth/password", "", body)
	challenge = out["challenge"].(string)
	status, _ = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": backups[0]})
	if status != 401 {
		t.Fatal("backup code replay accepted")
	}
	h.db.Exec(context.Background(), "UPDATE users SET google_sub='verified-test-sub' WHERE email='security@example.com'")
	w := httptest.NewRecorder()
	r := httptest.NewRequest("POST", "/verified-google-completion", nil)
	h.s.completeLogin(w, r, "security@example.com", "verified-test-sub", deviceInput{ID: deviceB, Name: "Password test", PublicKey: device["publicKey"].(string), Secret: device["secret"].(string)})
	if w.Code != 200 || !strings.Contains(w.Body.String(), "accessToken") {
		t.Fatal("Google login unexpectedly required application TOTP")
	}
	status, out = h.req("GET", "/v1/account", access, nil)
	if status != 200 || out["googleLinked"] != true || out["hasPassword"] != true || out["twoFactorEnabled"] != true || out["totpSecret"] != nil {
		t.Fatal("account security metadata incorrect")
	}
	status, _ = h.req("POST", "/v1/account/google/unlink", access, map[string]any{"currentPassword": password, "code": backups[1]})
	if status != 200 {
		t.Fatal("valid unlink failed")
	}
	status, out = h.req("GET", "/v1/account", access, nil)
	if out["googleLinked"] != false {
		t.Fatal("link state stale")
	}
	status, _ = h.req("POST", "/v1/account/2fa/disable", access, map[string]any{"currentPassword": password, "code": backups[2]})
	if status != 200 {
		t.Fatal("valid disable failed")
	}
	status, out = h.req("POST", "/v1/auth/password", "", body)
	if status != 200 || out["accessToken"] == nil {
		t.Fatal("password login after disable failed")
	}
}

func TestSecurityMutationsRequireProofAndFreshInitialAuthentication(t *testing.T) {
	h := setup(t)
	login := h.login("proof@example.com", deviceA)
	access := login["accessToken"].(string)
	h.db.Exec(context.Background(), "UPDATE users SET google_sub='proof-google' WHERE email='proof@example.com'")
	status, _ := h.req("POST", "/v1/account/google/unlink", access, map[string]any{"currentPassword": "", "code": ""})
	if status != 409 {
		t.Fatal("last working login method removed")
	}
	h.db.Exec(context.Background(), "UPDATE sessions SET auth_at=now()-interval '1 hour'")
	status, out := h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": login["refreshToken"]})
	access = out["accessToken"].(string)
	status, out = h.req("POST", "/v1/account/password", access, map[string]any{"password": "long-test-password", "currentPassword": "", "code": ""})
	if status != 403 || out["error"] != "reauthentication_required" {
		t.Fatal("refresh counted as fresh authentication")
	}
	h.db.Exec(context.Background(), "UPDATE sessions SET auth_at=now(),auth_method='email' WHERE NOT revoked")
	status, _ = h.req("POST", "/v1/account/password", access, map[string]any{"password": "long-test-password", "currentPassword": "", "code": ""})
	if status != 200 {
		t.Fatal("fresh initial authentication refused")
	}
	status, _ = h.req("POST", "/v1/account/password", access, map[string]any{"password": "another-long-password", "currentPassword": "incorrect", "code": ""})
	if status != 403 {
		t.Fatal("changed password without existing password")
	}
	status, _ = h.req("POST", "/v1/account/2fa/start", access, map[string]any{"currentPassword": "incorrect", "code": ""})
	if status != 403 {
		t.Fatal("enrolled TOTP without proof")
	}
}

func TestEnrollmentSurvivesRefreshButRemainsSessionBound(t *testing.T) {
	h := setup(t)
	login := h.login("enrollment@example.com", deviceA)
	access := login["accessToken"].(string)
	password := "test-password-enrollment"
	if status, _ := h.req("POST", "/v1/account/password", access, map[string]any{"password": password, "currentPassword": "", "code": ""}); status != 200 {
		t.Fatal("password setup failed")
	}
	other := h.login("enrollment@example.com", deviceB)
	status, out := h.req("POST", "/v1/account/2fa/start", access, map[string]any{"currentPassword": password, "code": ""})
	if status != 200 {
		t.Fatal("setup failed")
	}
	secret := out["secret"].(string)
	var stored string
	h.db.QueryRow(context.Background(), "SELECT secret FROM totp_setups").Scan(&stored)
	if strings.Contains(stored, secret) || stored == secret {
		t.Fatal("TOTP seed stored unencrypted")
	}
	if _, err := h.s.openTOTP("wrong-account", stored); err == nil {
		t.Fatal("TOTP account binding missing")
	}
	status, out = h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": login["refreshToken"]})
	if status != 200 {
		t.Fatal("refresh failed")
	}
	access = out["accessToken"].(string)
	code := totpCode(decodeTOTPSecret(secret), time.Now().Unix()/30, 6)
	if status, _ = h.req("POST", "/v1/account/2fa/enable", other["accessToken"].(string), map[string]any{"code": code}); status != 401 {
		t.Fatal("another session completed enrollment")
	}
	if status, _ = h.req("POST", "/v1/account/2fa/enable", access, map[string]any{"code": code}); status != 200 {
		t.Fatal("refresh broke enrollment")
	}
	if status, _ = h.req("GET", "/v1/account", other["accessToken"].(string), nil); status != 401 {
		t.Fatal("security change left another session active")
	}
	// Email OTP is a primary factor, not a bypass around enabled 2FA.
	email := "enrollment@example.com"
	h.db.Exec(context.Background(), "DELETE FROM auth_limits")
	status, _ = h.req("POST", "/v1/auth/email/request", "", map[string]any{"email": email})
	if status != 202 {
		t.Fatal("email code failed")
	}
	device := map[string]any{"id": deviceB, "name": "Test device", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceB[0])}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceB[0]) + 1}, 32))}
	status, out = h.req("POST", "/v1/auth/email/verify", "", map[string]any{"email": email, "code": h.mail.codes[email], "device": device})
	if status != 202 || out["twoFactorRequired"] != true || out["accessToken"] != nil {
		t.Fatal("email OTP bypassed 2FA")
	}
	challenge := out["challenge"].(string)
	h.db.Exec(context.Background(), "UPDATE login_challenges SET expires_at=now()-interval '1 second'")
	if status, _ = h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": code}); status != 401 {
		t.Fatal("expired login challenge accepted")
	}
}

func TestRefreshSerializesWithSecurityRevocation(t *testing.T) {
	h := setup(t)
	login := h.login("concurrent@example.com", deviceA)
	ctx := context.Background()
	tx, e := h.db.Begin(ctx)
	if e != nil {
		t.Fatal(e)
	}
	defer tx.Rollback(ctx)
	var id string
	if tx.QueryRow(ctx, "SELECT id::text FROM users WHERE email='concurrent@example.com' FOR UPDATE").Scan(&id) != nil {
		t.Fatal("lock failed")
	}
	done := make(chan int, 1)
	go func() {
		status, _ := h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": login["refreshToken"]})
		done <- status
	}()
	// Observe PostgreSQL's lock wait, rather than assuming a scheduling delay.
	deadline := time.Now().Add(3 * time.Second)
	waiting := false
	for time.Now().Before(deadline) {
		var n int
		h.db.QueryRow(ctx, "SELECT count(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT id% FROM users WHERE id=%FOR UPDATE'").Scan(&n)
		if n > 0 {
			waiting = true
			break
		}
		select {
		case <-done:
			t.Fatal("refresh did not take account lock")
		default:
		}
		time.Sleep(5 * time.Millisecond)
	}
	if !waiting {
		t.Fatal("refresh did not reach account lock")
	}
	if _, e = tx.Exec(ctx, "UPDATE users SET security_version=security_version+1 WHERE id=$1", id); e != nil {
		t.Fatal(e)
	}
	if _, e = tx.Exec(ctx, "UPDATE sessions SET revoked=true WHERE account_id=$1", id); e != nil {
		t.Fatal(e)
	}
	if e = tx.Commit(ctx); e != nil {
		t.Fatal(e)
	}
	select {
	case status := <-done:
		if status != 401 {
			t.Fatal("concurrent refresh survived security revocation")
		}
	case <-time.After(3 * time.Second):
		t.Fatal("refresh remained blocked")
	}
}

func TestFactorAttemptLimitAndPasswordBudget(t *testing.T) {
	h := setup(t)
	login := h.login("attempts@example.com", deviceA)
	access := login["accessToken"].(string)
	password := "test-password-attempts"
	h.req("POST", "/v1/account/password", access, map[string]any{"password": password, "currentPassword": "", "code": ""})
	_, out := h.req("POST", "/v1/account/2fa/start", access, map[string]any{"currentPassword": password, "code": ""})
	secret := out["secret"].(string)
	now := time.Now()
	h.s.now = func() time.Time { return now }
	h.req("POST", "/v1/account/2fa/enable", access, map[string]any{"code": totpCode(decodeTOTPSecret(secret), now.Unix()/30, 6)})
	device := map[string]any{"id": deviceA, "name": "Test device", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceA[0])}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceA[0]) + 1}, 32))}
	_, out = h.req("POST", "/v1/auth/password", "", map[string]any{"email": "attempts@example.com", "password": password, "device": device})
	challenge := out["challenge"].(string)
	for i := 0; i < 5; i++ {
		if status, _ := h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": "invalid"}); status != 401 {
			t.Fatal("wrong code accepted")
		}
	}
	now = now.Add(30 * time.Second)
	if status, _ := h.req("POST", "/v1/auth/2fa", "", map[string]any{"challenge": challenge, "code": totpCode(decodeTOTPSecret(secret), now.Unix()/30, 6)}); status != 401 {
		t.Fatal("attempt-limited challenge accepted")
	}
	passwordWorkers <- struct{}{}
	passwordWorkers <- struct{}{}
	defer func() { <-passwordWorkers; <-passwordWorkers }()
	if status, _ := h.req("POST", "/v1/auth/password", "", map[string]any{"email": "attempts@example.com", "password": password, "device": device}); status != 429 {
		t.Fatal("hashing budget did not reject excess work")
	}
}

type delayedBody struct{ started, release chan struct{} }

func (b *delayedBody) Read(p []byte) (int, error) { close(b.started); <-b.release; return 0, io.EOF }
func TestSlowBodiesDoNotReservePasswordBudget(t *testing.T) {
	h := setup(t)
	bodies := []*delayedBody{{make(chan struct{}), make(chan struct{})}, {make(chan struct{}), make(chan struct{})}}
	done := make(chan struct{}, 2)
	for _, body := range bodies {
		go func(body *delayedBody) {
			r := httptest.NewRequest(http.MethodPost, "/v1/auth/password", body)
			h.s.Handler().ServeHTTP(httptest.NewRecorder(), r)
			done <- struct{}{}
		}(body)
		<-body.started
	}
	defer func() {
		for _, body := range bodies {
			close(body.release)
		}
		<-done
		<-done
	}()
	device := map[string]any{"id": deviceA, "name": "Test device", "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "secret": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{2}, 32))}
	if status, _ := h.req("POST", "/v1/auth/password", "", map[string]any{"email": "missing@example.com", "password": "unconfigured-test-password", "device": device}); status != 401 {
		t.Fatal("slow body requests exhausted password budget")
	}
}

func TestLogoutAndRefreshLeaveNoLiveSession(t *testing.T) {
	h := setup(t)
	login := h.login("logout-race@example.com", deviceA)
	ctx := context.Background()
	tx, e := h.db.Begin(ctx)
	if e != nil {
		t.Fatal(e)
	}
	defer tx.Rollback(ctx)
	var id string
	tx.QueryRow(ctx, "SELECT id::text FROM users WHERE email='logout-race@example.com' FOR UPDATE").Scan(&id)
	done := make(chan int, 2)
	go func() {
		status, _ := h.req("POST", "/v1/auth/logout", login["accessToken"].(string), nil)
		done <- status
	}()
	go func() {
		status, _ := h.req("POST", "/v1/auth/refresh", "", map[string]any{"refreshToken": login["refreshToken"]})
		done <- status
	}()
	deadline := time.Now().Add(3 * time.Second)
	waiting := false
	for time.Now().Before(deadline) {
		var n int
		h.db.QueryRow(ctx, "SELECT count(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT id% FROM users WHERE id=%FOR UPDATE'").Scan(&n)
		if n == 2 {
			waiting = true
			break
		}
		select {
		case <-done:
			t.Fatal("logout or refresh did not serialize on account")
		default:
		}
		time.Sleep(5 * time.Millisecond)
	}
	if !waiting {
		t.Fatal("both handlers did not reach account lock")
	}
	tx.Commit(ctx)
	for i := 0; i < 2; i++ {
		select {
		case status := <-done:
			if status != 200 && status != 401 {
				t.Fatal("unexpected concurrent status")
			}
		case <-time.After(3 * time.Second):
			t.Fatal("handler blocked")
		}
	}
	var count int
	h.db.QueryRow(ctx, "SELECT count(*) FROM sessions WHERE account_id=$1 AND NOT revoked", id).Scan(&count)
	if count != 0 {
		t.Fatal("logout left a refreshed session alive")
	}
}
