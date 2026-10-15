package api

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
)

func TestMCPDiscoveryAndSeparateAudience(t *testing.T) {
	s := New(nil, Config{PublicURL: "https://nodecloak.test", TokenPepper: strings.Repeat("a", 64)}, nil)
	for path, key := range map[string]string{"/.well-known/oauth-protected-resource/mcp": "resource", "/.well-known/oauth-authorization-server": "code_challenge_methods_supported"} {
		w := httptest.NewRecorder()
		s.Handler().ServeHTTP(w, httptest.NewRequest("GET", path, nil))
		if w.Code != 200 || !strings.Contains(w.Body.String(), key) {
			t.Fatalf("discovery %s: %d %s", path, w.Code, w.Body)
		}
	}
	w := httptest.NewRecorder()
	s.Handler().ServeHTTP(w, httptest.NewRequest("POST", "/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"initialize"}`)))
	if w.Code != 401 || !strings.Contains(w.Header().Get("WWW-Authenticate"), "resource_metadata") {
		t.Fatalf("missing challenge: %d %v", w.Code, w.Header())
	}
}
func TestMCPLogoutAndBackgroundCleanup(t *testing.T) {
	h := setup(t)
	access := h.login("remote-logout@example.com", deviceA)["accessToken"].(string)
	h.enroll(access)
	var account string
	if e := h.db.QueryRow(context.Background(), "SELECT account_id::text FROM sessions WHERE access_hash=$1", h.s.hash(access)).Scan(&account); e != nil {
		t.Fatal(e)
	}
	client, route := token(), uuid()
	_, e := h.db.Exec(context.Background(), "INSERT INTO mcp_clients(id,name,redirects) VALUES($1,'QA',ARRAY['https://example.test/cb'])", client)
	if e != nil {
		t.Fatal(e)
	}
	_, e = h.db.Exec(context.Background(), "INSERT INTO mcp_routes(id,account_id,device_id,local_grant_id,client_id,scopes,tools,expires_at) VALUES($1,$2,$3,$4,$5,ARRAY['read'],'[]',now()+interval '1 hour')", route, account, deviceA, uuid(), client)
	if e != nil {
		t.Fatal(e)
	}
	if status, _ := h.req("POST", "/v1/auth/logout", access, map[string]any{}); status != 200 {
		t.Fatal(status)
	}
	var revoked bool
	if e = h.db.QueryRow(context.Background(), "SELECT revoked FROM mcp_routes WHERE id=$1", route).Scan(&revoked); e != nil || !revoked {
		t.Fatal("logout left remote route active", e)
	}
	session := token()
	_, e = h.db.Exec(context.Background(), "INSERT INTO mcp_sessions(id,route_id) VALUES($1,$2)", session, route)
	if e != nil {
		t.Fatal(e)
	}
	for _, state := range []string{"queued", "dispatched", "completed", "cancelled"} {
		_, e = h.db.Exec(context.Background(), "INSERT INTO mcp_jobs(id,route_id,session_id,request_id,tool,argument_hash,arguments_sealed,result_sealed,state,expires_at) VALUES($1,$2,$3,$4,'profiles_list','hash','old-ciphertext','old-result',$4,now()-interval '1 second')", uuid(), route, session, state)
		if e != nil {
			t.Fatal(e)
		}
	}
	if e = h.s.Cleanup(context.Background()); e != nil {
		t.Fatal(e)
	}
	var count int
	_ = h.db.QueryRow(context.Background(), "SELECT count(*) FROM mcp_jobs WHERE arguments_sealed<>'' OR result_sealed<>'' OR state IN('queued','dispatched')").Scan(&count)
	if count != 0 {
		t.Fatal("expired payload/work retained", count)
	}
}
func TestMCPRelayCiphertextIsBoundToJob(t *testing.T) {
	s := New(nil, Config{TokenPepper: strings.Repeat("a", 64)}, nil)
	sealed, e := s.mcpSeal("arguments:one", "private-value")
	if e != nil {
		t.Fatal(e)
	}
	if strings.Contains(sealed, "private-value") {
		t.Fatal("plaintext persisted")
	}
	if _, e = s.mcpOpen("arguments:two", sealed); e == nil {
		t.Fatal("ciphertext substitution accepted")
	}
	if _, e = s.mcpOpen("result:one", sealed); e == nil {
		t.Fatal("direction substitution accepted")
	}
}
func mcpForm(h *harness, path string, form url.Values) (int, map[string]any) {
	r := httptest.NewRequest("POST", path, strings.NewReader(form.Encode()))
	r.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, r)
	var out map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &out)
	return w.Code, out
}
func mcpBegin(h *harness) (string, string, string, string) {
	h.Helper()
	status, client := h.req("POST", "/mcp/oauth/register", "", map[string]any{"client_name": "QA AI", "redirect_uris": []string{"http://127.0.0.1:53281/callback"}, "token_endpoint_auth_method": "none", "grant_types": []string{"authorization_code", "refresh_token"}, "response_types": []string{"code"}})
	if status != 201 {
		h.Fatalf("register %d %v", status, client)
	}
	verifier := strings.Repeat("v", 43)
	sum := sha256.Sum256([]byte(verifier))
	challenge := base64.RawURLEncoding.EncodeToString(sum[:])
	cid := client["client_id"].(string)
	query := url.Values{"response_type": {"code"}, "client_id": {cid}, "redirect_uri": {"http://127.0.0.1:53281/callback"}, "code_challenge": {challenge}, "code_challenge_method": {"S256"}, "resource": {h.s.cfg.PublicURL + "/mcp"}, "scope": {"read page_read page_write"}, "state": {"caller-state"}}
	w := httptest.NewRecorder()
	h.s.Handler().ServeHTTP(w, httptest.NewRequest("GET", "/mcp/oauth/authorize?"+query.Encode(), nil))
	if w.Code != 200 {
		h.Fatalf("authorize %d %s", w.Code, w.Body)
	}
	var code, poll string
	if e := h.db.QueryRow(context.Background(), "SELECT user_code,poll_token FROM mcp_authorizations WHERE client_id=$1 ORDER BY created_at DESC LIMIT 1", cid).Scan(&code, &poll); e != nil {
		h.Fatal(e)
	}
	return cid, verifier, code, poll
}
func TestMCPOAuthPKCEReplayRotationAndRelay(t *testing.T) {
	h := setup(t)
	accountToken := h.login("mcp@example.com", deviceA)["accessToken"].(string)
	h.enroll(accountToken)
	cid, verifier, code, poll := mcpBegin(h)
	status, pending := h.req("GET", "/v1/mcp/requests/"+code, accountToken, nil)
	if status != 200 || pending["client_name"] != "QA AI" {
		t.Fatal(status, pending)
	}
	tools := []any{map[string]any{"name": "profiles_list", "description": "List profiles", "inputSchema": map[string]any{"type": "object", "properties": map[string]any{}}}}
	status, approved := h.req("POST", "/v1/mcp/approve", accountToken, map[string]any{"user_code": code, "local_grant_id": objectA, "scopes": []string{"read"}, "expires_in": 3600, "tools": tools})
	if status != 200 {
		t.Fatal(status, approved)
	}
	status, p := h.req("GET", "/mcp/oauth/pending/"+poll, "", nil)
	if status != 200 {
		t.Fatal(status, p)
	}
	u, e := url.Parse(p["redirect"].(string))
	if e != nil || u.Query().Get("state") != "caller-state" {
		t.Fatal(p, e)
	}
	form := url.Values{"grant_type": {"authorization_code"}, "client_id": {cid}, "code": {u.Query().Get("code")}, "code_verifier": {"wrong"}, "redirect_uri": {"http://127.0.0.1:53281/callback"}, "resource": {h.s.cfg.PublicURL + "/mcp"}}
	form.Set("resource", h.s.cfg.PublicURL+"/v1")
	if status, _ := mcpForm(h, "/mcp/oauth/token", form); status != 400 {
		t.Fatal("wrong audience accepted", status)
	}
	form.Set("resource", h.s.cfg.PublicURL+"/mcp")
	form.Set("redirect_uri", "http://127.0.0.1:53281/another")
	if status, _ := mcpForm(h, "/mcp/oauth/token", form); status != 400 {
		t.Fatal("unregistered redirect accepted", status)
	}
	form.Set("redirect_uri", "http://127.0.0.1:53281/callback")
	if status, _ := mcpForm(h, "/mcp/oauth/token", form); status != 400 {
		t.Fatal("wrong verifier accepted", status)
	}
	form.Set("code_verifier", verifier)
	status, tokens := mcpForm(h, "/mcp/oauth/token", form)
	if status != 200 {
		t.Fatal(status, tokens)
	}
	if status, _ := mcpForm(h, "/mcp/oauth/token", form); status != 400 {
		t.Fatal("authorization code replay", status)
	}
	bearer := tokens["access_token"].(string)
	route := approved["route_id"].(string)
	status, _ = h.req("POST", "/v1/mcp/relay", accountToken, map[string]any{"routes": []string{route}})
	if status != 200 {
		t.Fatal(status)
	}
	call := func(session, method string, id int, params any) (int, map[string]any, string) {
		b, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": id, "method": method, "params": params})
		r := httptest.NewRequest("POST", "/mcp", bytes.NewReader(b))
		r.Header.Set("Authorization", "Bearer "+bearer)
		r.Header.Set("Content-Type", "application/json")
		r.Header.Set("Accept", "application/json, text/event-stream")
		if session != "" {
			r.Header.Set("Mcp-Session-Id", session)
		}
		w := httptest.NewRecorder()
		h.s.Handler().ServeHTTP(w, r)
		var v map[string]any
		_ = json.Unmarshal(w.Body.Bytes(), &v)
		return w.Code, v, w.Header().Get("Mcp-Session-Id")
	}
	status, _, session := call("", "initialize", 1, map[string]any{"protocolVersion": "2025-11-25", "capabilities": map[string]any{}, "clientInfo": map[string]any{"name": "QA", "version": "1"}})
	if status != 200 || session == "" {
		t.Fatal("initialize", status)
	}
	status, result, _ := call(session, "tools/call", 2, map[string]any{"name": "profiles_list", "arguments": map[string]any{}})
	if status != 200 {
		t.Fatal(status, result)
	}
	job := result["result"].(map[string]any)["structuredContent"].(map[string]any)
	id := job["id"].(string)
	_, again, _ := call(session, "tools/call", 2, map[string]any{"name": "profiles_list", "arguments": map[string]any{}})
	if fmt.Sprint(again["result"].(map[string]any)["structuredContent"].(map[string]any)["id"]) != id {
		t.Fatal("duplicate created new job")
	}
	if status, _, _ := call(session, "tools/call", 2, map[string]any{"name": "profiles_list", "arguments": map[string]any{"changed": true}}); status != 409 {
		t.Fatal("altered duplicate arguments accepted", status)
	}
	other := h.login("other-mcp@example.com", deviceB)["accessToken"].(string)
	h.enroll(other)
	if status, foreign := h.req("POST", "/v1/mcp/relay", other, map[string]any{"routes": []string{route}}); status != 200 || len(foreign["jobs"].([]any)) != 0 || len(foreign["routes"].([]any)) != 0 {
		t.Fatal("another account claimed route", status, foreign)
	}
	status, claimed := h.req("POST", "/v1/mcp/relay", accountToken, map[string]any{"routes": []string{route}})
	if status != 200 || len(claimed["jobs"].([]any)) != 1 {
		t.Fatal(status, claimed)
	}
	_, second := h.req("POST", "/v1/mcp/relay", accountToken, map[string]any{"routes": []string{route}})
	if len(second["jobs"].([]any)) != 0 {
		t.Fatal("job redelivered", second)
	}
	for _, state := range []string{"running", "awaiting_approval"} {
		if status, _ := h.req("POST", "/v1/mcp/relay/"+id, accountToken, map[string]any{"route_id": route, "state": state}); status != 200 {
			t.Fatal(status)
		}
		job, e := h.s.mcpJob(httptest.NewRequest("GET", "/", nil), route, id)
		if e != nil || job["state"] != state || job["result"] != nil {
			t.Fatal("missing result poisoned polling", job, e)
		}
	}
	if status, _ := h.req("POST", "/v1/mcp/relay/"+id, accountToken, map[string]any{"route_id": route, "state": "completed", "result": map[string]any{"profiles": []any{}}, "error": ""}); status != 200 {
		t.Fatal(status)
	}
	if status, _ := h.req("POST", "/v1/mcp/relay/"+id, other, map[string]any{"route_id": route, "state": "completed", "result": map[string]any{"foreign": true}}); status != 409 {
		t.Fatal("another account updated job", status)
	}
	_, queued, _ := call(session, "tools/call", 4, map[string]any{"name": "profiles_list", "arguments": map[string]any{}})
	cancelID := queued["result"].(map[string]any)["structuredContent"].(map[string]any)["id"].(string)
	_, cancelled, _ := call(session, "tools/call", 5, map[string]any{"name": "jobs_cancel", "arguments": map[string]any{"job_id": cancelID}})
	if cancelled["result"].(map[string]any)["structuredContent"].(map[string]any)["state"] != "cancelled" {
		t.Fatal("queued cancellation failed", cancelled)
	}
	_, afterCancel := h.req("POST", "/v1/mcp/relay", accountToken, map[string]any{"routes": []string{route}})
	if len(afterCancel["jobs"].([]any)) != 0 {
		t.Fatal("cancelled request dispatched", afterCancel)
	}
	call(session, "tools/call", 6, map[string]any{"name": "profiles_list", "arguments": map[string]any{}})
	claims := make(chan int, 2)
	for range 2 {
		go func() {
			_, v := h.req("POST", "/v1/mcp/relay", accountToken, map[string]any{"routes": []string{route}})
			claims <- len(v["jobs"].([]any))
		}()
	}
	if total := <-claims + <-claims; total != 1 {
		t.Fatal("concurrent relay dispatch count", total)
	}
	refresh := url.Values{"grant_type": {"refresh_token"}, "client_id": {cid}, "refresh_token": {tokens["refresh_token"].(string)}, "resource": {h.s.cfg.PublicURL + "/mcp"}}
	if status, _ := mcpForm(h, "/mcp/oauth/token", refresh); status != 200 {
		t.Fatal("refresh failed", status)
	}
	if status, _ := mcpForm(h, "/mcp/oauth/token", refresh); status != 400 {
		t.Fatal("refresh replay accepted", status)
	}
	if status, _, _ := call(session, "tools/list", 3, nil); status != 401 {
		t.Fatal("reuse failed to revoke grant", status)
	}
}
