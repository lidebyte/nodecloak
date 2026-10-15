package api

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"
)

type mcpRevocationFixture struct {
	account, device, client, route, access, refresh, session, completed string
}

func seedMCPRevocationFixture(h *harness, desktop, device string) mcpRevocationFixture {
	h.Helper()
	ctx := context.Background()
	f := mcpRevocationFixture{device: device, client: token(), route: uuid(), access: token(), refresh: token(), session: token()}
	if e := h.db.QueryRow(ctx, "SELECT account_id::text FROM sessions WHERE access_hash=$1", h.s.hash(desktop)).Scan(&f.account); e != nil {
		h.Fatal(e)
	}
	exec := func(sql string, args ...any) {
		h.Helper()
		if _, e := h.db.Exec(ctx, sql, args...); e != nil {
			h.Fatal(e)
		}
	}
	exec("INSERT INTO mcp_clients(id,name,redirects) VALUES($1,'Revocation QA',ARRAY['https://example.test/cb'])", f.client)
	exec("INSERT INTO mcp_routes(id,account_id,device_id,local_grant_id,client_id,scopes,tools,expires_at,last_seen) VALUES($1,$2,$3,$4,$5,ARRAY['read'],'[]',now()+interval '1 hour',now())", f.route, f.account, device, uuid(), f.client)
	exec("INSERT INTO mcp_tokens(access_hash,refresh_hash,route_id) VALUES($1,$2,$3)", h.s.hash(f.access), h.s.hash(f.refresh), f.route)
	exec("INSERT INTO mcp_sessions(id,route_id) VALUES($1,$2)", f.session, f.route)
	for _, state := range []string{"queued", "dispatched", "running", "awaiting_approval", "completed", "failed", "cancelled"} {
		job := uuid()
		if state == "completed" {
			f.completed = job
		}
		result, e := h.s.mcpSeal("result:"+job, `{"text":"synthetic-private-page"}`)
		if e != nil {
			h.Fatal(e)
		}
		args, e := h.s.mcpSeal("arguments:"+job, `{"url":"https://example.test/private"}`)
		if e != nil {
			h.Fatal(e)
		}
		exec("INSERT INTO mcp_jobs(id,route_id,session_id,request_id,tool,argument_hash,arguments_sealed,result_sealed,state) VALUES($1,$2,$3,$4,'page_snapshot','hash',$5,$6,$4)", job, f.route, f.session, state, args, result)
	}
	return f
}

func (f mcpRevocationFixture) read(h *harness) (int, string) {
	h.Helper()
	r := httptest.NewRequest("POST", "/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"jobs_get","arguments":{"job_id":"`+f.completed+`"}}}`))
	r.Header.Set("Authorization", "Bearer "+f.access)
	r.Header.Set("Content-Type", "application/json")
	r.Header.Set("Mcp-Session-Id", f.session)
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, r)
	return w.Code, w.Body.String()
}

func (f mcpRevocationFixture) assertRevoked(h *harness) {
	h.Helper()
	if code, body := f.read(h); code != 401 {
		h.Errorf("revoked MCP token can still read jobs: %d %s", code, body)
	}
	status, body := mcpForm(h, "/mcp/oauth/token", url.Values{"grant_type": {"refresh_token"}, "client_id": {f.client}, "refresh_token": {f.refresh}, "resource": {h.s.mcpResource()}})
	if status != 400 || body["error"] != "invalid_grant" {
		h.Errorf("revoked MCP refresh accepted: %d", status)
	}
	var private, active int
	if e := h.db.QueryRow(context.Background(), "SELECT count(*) FILTER(WHERE arguments_sealed<>'' OR result_sealed<>''),count(*) FILTER(WHERE state IN('queued','dispatched','running','awaiting_approval') AND NOT cancel_requested) FROM mcp_jobs WHERE route_id=$1", f.route).Scan(&private, &active); e != nil {
		h.Fatal(e)
	}
	if private != 0 || active != 0 {
		h.Errorf("revocation retained private payloads or executable jobs: private=%d active=%d", private, active)
	}
}

func (f mcpRevocationFixture) assertActive(h *harness) {
	h.Helper()
	if code, body := f.read(h); code != 200 || !strings.Contains(body, "synthetic-private-page") {
		h.Errorf("unrelated MCP route was affected: %d %s", code, body)
	}
}

func TestMCPAccountAndDeviceRevocation(t *testing.T) {
	for _, action := range []string{"admin-user", "admin-device", "suspend", "logout", "password", "rotate"} {
		t.Run(action, func(t *testing.T) {
			h := setup(t)
			first := h.login("revoke-owner@example.com", deviceA)["accessToken"].(string)
			h.enroll(first)
			second := h.login("revoke-owner@example.com", deviceB)["accessToken"].(string)
			var account string
			if e := h.db.QueryRow(context.Background(), "SELECT account_id::text FROM sessions WHERE access_hash=$1", h.s.hash(first)).Scan(&account); e != nil {
				t.Fatal(e)
			}
			// Device approval is fixture setup; revocation below uses the production HTTP endpoint.
			if _, e := h.db.Exec(context.Background(), "UPDATE devices SET status='approved',epoch=1 WHERE account_id=$1 AND id=$2", account, deviceB); e != nil {
				t.Fatal(e)
			}
			unrelated := h.login("revoke-other@example.com", deviceA)["accessToken"].(string)
			h.enroll(unrelated)
			a, b, other := seedMCPRevocationFixture(h, first, deviceA), seedMCPRevocationFixture(h, second, deviceB), seedMCPRevocationFixture(h, unrelated, deviceA)
			b.assertActive(h)
			all := action == "admin-user" || action == "suspend" || action == "password"
			status := 0
			var out map[string]any
			switch action {
			case "admin-user", "admin-device", "suspend":
				admin := newAdminClient(t, h, "revoke-admin@example.com")
				path := "/v1/admin/users/" + account + "/sessions/revoke"
				if action == "admin-device" {
					path = "/v1/admin/devices/" + account + "/" + deviceB + "/sessions/revoke"
				}
				if action == "suspend" {
					status, out = admin.req("PATCH", "/v1/admin/users/"+account, map[string]any{"disabled": true}, admin.csrf, "")
					if status != 200 {
						t.Fatal(status, out)
					}
					status, out = admin.req("PATCH", "/v1/admin/users/"+account, map[string]any{"disabled": false}, admin.csrf, "")
				} else {
					status, out = admin.req("POST", path, map[string]any{}, admin.csrf, "")
				}
			case "logout":
				status, out = h.req("POST", "/v1/auth/logout", second, map[string]any{})
			case "password":
				status, out = h.req("POST", "/v1/account/password", first, map[string]any{"password": "revocation-test-password"})
			case "rotate":
				key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{7}, 32))
				status, out = h.req("POST", "/v1/vault/rotate", first, map[string]any{"baseEpoch": 1, "revokeDevice": deviceB, "recoveryEnvelope": "replacement-recovery-envelope", "recoveryVerifier": base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey)), "devices": []any{map[string]any{"id": deviceA, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "replacement-device-envelope"}}, "objects": []any{}})
			}
			if status != 200 {
				t.Fatal(status, out)
			}
			if code, _ := h.req("GET", "/v1/account", second, nil); code != 401 {
				t.Errorf("desktop session still active: %d", code)
			}
			b.assertRevoked(h)
			if all {
				a.assertRevoked(h)
			} else {
				a.assertActive(h)
			}
			other.assertActive(h)
			if action != "rotate" {
				// Signing into the same device must not revive the old remote authorization.
				if _, e := h.db.Exec(context.Background(), "UPDATE auth_limits SET started_at=now()-interval '2 minutes' WHERE key=$1", h.s.hash("email:revoke-owner@example.com")); e != nil {
					t.Fatal(e)
				}
				current := h.login("revoke-owner@example.com", deviceB)["accessToken"].(string)
				if code, _ := h.req("GET", "/v1/account", current, nil); code != 200 {
					t.Fatal("new login failed", code)
				}
				b.assertRevoked(h)
				code, relay := h.req("POST", "/v1/mcp/relay", current, map[string]any{"routes": []string{b.route}})
				if code != 200 {
					t.Fatal(code, relay)
				}
				if len(relay["routes"].([]any)) != 0 || len(relay["jobs"].([]any)) != 0 {
					t.Fatal("revoked route resumed after login", relay)
				}
			}
		})
	}
}

func TestMCPApprovalRejectsRevokedDesktopIdentity(t *testing.T) {
	h := setup(t)
	access := h.login("approve-revoke@example.com", deviceA)["accessToken"].(string)
	h.enroll(access)
	var i identity
	if e := h.db.QueryRow(context.Background(), "SELECT account_id::text,device_id::text,id::text FROM sessions WHERE access_hash=$1", h.s.hash(access)).Scan(&i.Account, &i.Device, &i.Session); e != nil {
		t.Fatal(e)
	}
	_, _, code, _ := mcpBegin(h)
	if status, out := h.req("POST", "/v1/auth/logout", access, map[string]any{}); status != 200 {
		t.Fatal(status, out)
	}
	body, _ := json.Marshal(map[string]any{"user_code": code, "local_grant_id": uuid(), "scopes": []string{"read"}, "expires_in": 3600, "tools": []any{}})
	// Model a request that passed middleware before logout, then acquired its write lock afterwards.
	r := httptest.NewRequest("POST", "/v1/mcp/approve", bytes.NewReader(body))
	w := httptest.NewRecorder()
	h.s.mcpApprove(w, r, i)
	if w.Code != 401 {
		t.Fatalf("in-flight approval accepted after session revocation: %d %s", w.Code, w.Body)
	}
	var count int
	if e := h.db.QueryRow(context.Background(), "SELECT count(*) FROM mcp_routes WHERE account_id=$1", i.Account).Scan(&count); e != nil || count != 0 {
		t.Fatal("stale approval created a route", count, e)
	}
}

func TestMCPRevocationRejectsInFlightResultWrite(t *testing.T) {
	h := setup(t)
	access := h.login("result-revoke@example.com", deviceA)["accessToken"].(string)
	h.enroll(access)
	f := seedMCPRevocationFixture(h, access, deviceA)
	var job string
	if e := h.db.QueryRow(context.Background(), "SELECT id::text FROM mcp_jobs WHERE route_id=$1 AND state='running'", f.route).Scan(&job); e != nil {
		t.Fatal(e)
	}
	tx, e := h.db.Begin(context.Background())
	if e != nil {
		t.Fatal(e)
	}
	defer tx.Rollback(context.Background())
	// Hold the same account lock and uncommitted invalidation as logout/admin revoke.
	if _, e = tx.Exec(context.Background(), "SELECT id FROM users WHERE id=$1 FOR UPDATE", f.account); e != nil {
		t.Fatal(e)
	}
	if _, e = tx.Exec(context.Background(), "UPDATE sessions SET revoked=true WHERE account_id=$1", f.account); e != nil {
		t.Fatal(e)
	}
	if e = revokeDeviceMCP(context.Background(), tx, f.account, ""); e != nil {
		t.Fatal(e)
	}
	body, _ := json.Marshal(map[string]any{"route_id": f.route, "state": "completed", "result": map[string]any{"text": "late private result"}})
	r := httptest.NewRequest("POST", "/v1/mcp/relay/"+job, bytes.NewReader(body))
	r.Header.Set("Authorization", "Bearer "+access)
	r.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	done := make(chan struct{})
	go func() { defer close(done); h.s.Handler().ServeHTTP(w, r) }()
	// Wait for the real write to block on revocation; do not depend on scheduling sleeps.
	deadline := time.Now().Add(3 * time.Second)
	for {
		var waiting bool
		if e = h.db.QueryRow(context.Background(), "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock')").Scan(&waiting); e != nil {
			t.Fatal(e)
		}
		if waiting {
			break
		}
		select {
		case <-done:
			t.Fatalf("write did not reach the authorization lock: %d %s", w.Code, w.Body)
		default:
		}
		if time.Now().After(deadline) {
			t.Fatal("write never waited on revocation")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if e = tx.Commit(context.Background()); e != nil {
		t.Fatal(e)
	}
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("result write did not finish")
	}
	if w.Code != 401 {
		t.Errorf("in-flight result survived session revocation: %d %s", w.Code, w.Body)
	}
	f.assertRevoked(h)
}
