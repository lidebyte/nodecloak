package api

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"html/template"
	"net/http"
	"net/url"
	"regexp"
	"slices"
	"strings"
)

var mcpScopes = []string{"read", "manage", "page_read", "page_write", "files", "sync"}

func (s *Server) mcpResource() string { return strings.TrimRight(s.cfg.PublicURL, "/") + "/mcp" }
func (s *Server) registerMCP() {
	s.mux.HandleFunc("GET /.well-known/oauth-protected-resource/mcp", func(w http.ResponseWriter, r *http.Request) {
		reply(w, 200, map[string]any{"resource": s.mcpResource(), "authorization_servers": []string{s.cfg.PublicURL}, "scopes_supported": []string{"read"}, "bearer_methods_supported": []string{"header"}})
	})
	s.mux.HandleFunc("GET /.well-known/oauth-authorization-server", func(w http.ResponseWriter, r *http.Request) {
		base := s.mcpResource() + "/oauth/"
		reply(w, 200, map[string]any{"issuer": s.cfg.PublicURL, "authorization_endpoint": base + "authorize", "token_endpoint": base + "token", "registration_endpoint": base + "register", "revocation_endpoint": base + "revoke", "response_types_supported": []string{"code"}, "grant_types_supported": []string{"authorization_code", "refresh_token"}, "token_endpoint_auth_methods_supported": []string{"none"}, "code_challenge_methods_supported": []string{"S256"}, "scopes_supported": mcpScopes})
	})
	s.mux.HandleFunc("POST /mcp/oauth/register", s.mcpRegister)
	s.mux.HandleFunc("GET /mcp/oauth/authorize", s.mcpAuthorize)
	s.mux.HandleFunc("GET /mcp/oauth/pending/{poll}", s.mcpPending)
	s.mux.HandleFunc("POST /mcp/oauth/token", s.mcpToken)
	s.mux.HandleFunc("POST /mcp/oauth/revoke", s.mcpRevoke)
	s.mux.HandleFunc("GET /v1/mcp/requests/{code}", s.auth(true, s.mcpRequest))
	s.mux.HandleFunc("POST /v1/mcp/approve", s.auth(true, s.mcpApprove))
	s.mux.HandleFunc("POST /v1/mcp/relay", s.auth(true, s.mcpRelay))
	s.mux.HandleFunc("POST /v1/mcp/relay/{id}", s.auth(true, s.mcpUpdate))
	s.mux.HandleFunc("POST /v1/mcp/routes/{id}/revoke", s.auth(true, s.mcpRouteRevoke))
	s.mux.HandleFunc("POST /mcp", s.mcpHTTP)
	s.mux.HandleFunc("GET /mcp", func(w http.ResponseWriter, r *http.Request) { w.Header().Set("Allow", "POST"); w.WriteHeader(405) })
}
func (s *Server) mcpBrowserOrigin(r *http.Request) bool {
	if !(strings.HasPrefix(r.URL.Path, "/mcp/oauth/") || r.URL.Path == "/mcp") {
		return false
	}
	u, e := url.Parse(s.cfg.PublicURL)
	return e == nil && r.Header.Get("Origin") == u.Scheme+"://"+u.Host
}
func mcpRedirect(v string) bool {
	u, e := url.Parse(v)
	return e == nil && len(v) <= 2048 && u.Host != "" && u.User == nil && u.Fragment == "" && (u.Scheme == "https" || (u.Scheme == "http" && (u.Hostname() == "127.0.0.1" || u.Hostname() == "localhost" || u.Hostname() == "::1"))) && u.Query().Get("code") == "" && u.Query().Get("state") == ""
}
func (s *Server) mcpRegister(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Name      string   `json:"client_name"`
		Redirects []string `json:"redirect_uris"`
		Auth      string   `json:"token_endpoint_auth_method"`
		Grants    []string `json:"grant_types"`
		Responses []string `json:"response_types"`
		URI       string   `json:"client_uri"`
		Logo      string   `json:"logo_uri"`
	}
	if !decode(w, r, &in, 16384) {
		return
	}
	if len(in.Name) < 1 || len(in.Name) > 100 || len(in.Redirects) < 1 || len(in.Redirects) > 10 || (in.Auth != "" && in.Auth != "none") {
		failure(w, 400, "invalid_client_metadata")
		return
	}
	for _, v := range in.Redirects {
		if !mcpRedirect(v) {
			failure(w, 400, "invalid_redirect_uri")
			return
		}
	}
	for _, v := range in.Grants {
		if v != "authorization_code" && v != "refresh_token" {
			failure(w, 400, "invalid_client_metadata")
			return
		}
	}
	for _, v := range in.Responses {
		if v != "code" {
			failure(w, 400, "invalid_client_metadata")
			return
		}
	}
	if !s.limit(r.Context(), "mcp-register:"+s.clientIP(r), 5) {
		failure(w, 429, "try_later")
		return
	}
	id := token()
	registered, e := s.db.Exec(r.Context(), "INSERT INTO mcp_clients(id,name,redirects) SELECT $1,$2,$3 WHERE (SELECT count(*) FROM mcp_clients)<10000", id, in.Name, in.Redirects)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if registered.RowsAffected() != 1 {
		failure(w, 429, "registration_limit")
		return
	}
	reply(w, 201, map[string]any{"client_id": id, "client_name": in.Name, "redirect_uris": in.Redirects, "token_endpoint_auth_method": "none", "grant_types": []string{"authorization_code", "refresh_token"}, "response_types": []string{"code"}})
}

var mcpPKCE = regexp.MustCompile(`^[A-Za-z0-9_-]{43}$`)
var mcpVerifier = regexp.MustCompile(`^[A-Za-z0-9._~-]{43,128}$`)

func (s *Server) mcpAuthorize(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	scopes := strings.Fields(q.Get("scope"))
	if len(scopes) == 0 {
		scopes = []string{"read"}
	}
	var redirects []string
	var name string
	if q.Get("response_type") != "code" || q.Get("resource") != s.mcpResource() || q.Get("code_challenge_method") != "S256" || !mcpPKCE.MatchString(q.Get("code_challenge")) || len(q.Get("state")) > 1024 || s.db.QueryRow(r.Context(), "SELECT name,redirects FROM mcp_clients WHERE id=$1", q.Get("client_id")).Scan(&name, &redirects) != nil || !slices.Contains(redirects, q.Get("redirect_uri")) {
		failure(w, 400, "invalid_request")
		return
	}
	for _, v := range scopes {
		if !slices.Contains(mcpScopes, v) {
			failure(w, 400, "invalid_scope")
			return
		}
	}
	if !s.limit(r.Context(), "mcp-authorize:"+s.clientIP(r), 20) {
		failure(w, 429, "try_later")
		return
	}
	code := strings.ToUpper(strings.ReplaceAll(uuid(), "-", ""))[:12]
	poll := token()
	_, e := s.db.Exec(r.Context(), "INSERT INTO mcp_authorizations(id,client_id,redirect_uri,state,challenge,resource,scopes,user_code,poll_token) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", uuid(), q.Get("client_id"), q.Get("redirect_uri"), q.Get("state"), q.Get("code_challenge"), q.Get("resource"), scopes, code, poll)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	renderMCPConsent(w, name, code, poll)
}
func (s *Server) mcpPending(w http.ResponseWriter, r *http.Request) {
	var redirect, state, id, sealed string
	e := s.db.QueryRow(r.Context(), "SELECT id::text,redirect_uri,state,coalesce(code_sealed,'') FROM mcp_authorizations WHERE poll_token=$1 AND expires_at>now() AND NOT redeemed", r.PathValue("poll")).Scan(&id, &redirect, &state, &sealed)
	if e != nil {
		failure(w, 410, "authorization_expired")
		return
	}
	if sealed == "" {
		reply(w, 200, map[string]any{"pending": true})
		return
	}
	code, e := s.mcpOpen("authorization:"+id, sealed)
	if e != nil {
		failure(w, 503, "authorization_unavailable")
		return
	}
	u, _ := url.Parse(redirect)
	v := u.Query()
	v.Set("code", code)
	v.Set("state", state)
	u.RawQuery = v.Encode()
	reply(w, 200, map[string]any{"redirect": u.String()})
}
func (s *Server) mcpRequest(w http.ResponseWriter, r *http.Request, i identity) {
	code := strings.ToUpper(strings.ReplaceAll(r.PathValue("code"), "-", ""))
	if len(code) != 12 || !s.limit(r.Context(), "mcp-pair:"+i.Account, 10) {
		failure(w, 429, "try_later")
		return
	}
	var name, redirect string
	var scopes []string
	e := s.db.QueryRow(r.Context(), "SELECT c.name,a.redirect_uri,a.scopes FROM mcp_authorizations a JOIN mcp_clients c ON c.id=a.client_id WHERE a.user_code=$1 AND a.expires_at>now() AND a.grant_id IS NULL", code).Scan(&name, &redirect, &scopes)
	if e != nil {
		failure(w, 404, "authorization_not_found")
		return
	}
	reply(w, 200, map[string]any{"user_code": code, "client_name": name, "redirect_uri": redirect, "scopes": scopes, "resource": s.mcpResource()})
}
func (s *Server) mcpApprove(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		Code    string            `json:"user_code"`
		Local   string            `json:"local_grant_id"`
		Scopes  []string          `json:"scopes"`
		Expires int               `json:"expires_in"`
		Tools   []json.RawMessage `json:"tools"`
	}
	if !decode(w, r, &in, 262144) {
		return
	}
	in.Code = strings.ToUpper(strings.ReplaceAll(in.Code, "-", ""))
	if !validID(in.Local) || in.Expires < 60 || in.Expires > 90*86400 || len(in.Tools) > 64 || !slices.Contains(in.Scopes, "read") {
		failure(w, 400, "invalid_request")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	// Recheck under the account lock: logout or an administrator may have
	// revoked the desktop identity after the request passed middleware.
	if s.authorizeTx(r.Context(), tx, i, true) != nil {
		failure(w, 401, "unauthorized")
		return
	}
	var id, cid string
	var requested []string
	if tx.QueryRow(r.Context(), "SELECT id::text,client_id,scopes FROM mcp_authorizations WHERE user_code=$1 AND expires_at>now() AND grant_id IS NULL FOR UPDATE", in.Code).Scan(&id, &cid, &requested) != nil {
		failure(w, 400, "authorization_not_found")
		return
	}
	for _, v := range in.Scopes {
		if !slices.Contains(mcpScopes, v) || !slices.Contains(requested, v) {
			failure(w, 400, "invalid_scope")
			return
		}
	}
	filtered := []json.RawMessage{}
	for _, raw := range in.Tools {
		var t struct {
			Name string `json:"name"`
		}
		if json.Unmarshal(raw, &t) != nil || mcpPermission(t.Name) == "" {
			failure(w, 400, "invalid_tools")
			return
		}
		if slices.Contains(in.Scopes, mcpPermission(t.Name)) {
			filtered = append(filtered, raw)
		}
	}
	route, code := uuid(), token()
	sealed, e := s.mcpSeal("authorization:"+id, code)
	if e == nil {
		_, e = tx.Exec(r.Context(), "INSERT INTO mcp_routes(id,account_id,device_id,local_grant_id,client_id,scopes,tools,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,now()+$8*interval '1 second')", route, i.Account, i.Device, in.Local, cid, in.Scopes, filtered, in.Expires)
	}
	if e == nil {
		_, e = tx.Exec(r.Context(), "UPDATE mcp_authorizations SET grant_id=$2,code_hash=$3,code_sealed=$4,expires_at=now()+interval '5 minutes' WHERE id=$1", id, route, s.hash(code), sealed)
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"route_id": route, "resource": s.mcpResource()})
}
func (s *Server) mcpToken(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, 8192)
	if r.ParseForm() != nil {
		failure(w, 400, "invalid_request")
		return
	}
	f := r.PostForm
	if f.Get("resource") != s.mcpResource() {
		failure(w, 400, "invalid_target")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var route string
	if f.Get("grant_type") == "authorization_code" {
		var challenge, redirect, cid string
		var redeemed bool
		e = tx.QueryRow(r.Context(), "SELECT a.grant_id::text,a.challenge,a.redirect_uri,a.client_id,a.redeemed FROM mcp_authorizations a WHERE a.code_hash=$1 AND a.expires_at>now() FOR UPDATE", s.hash(f.Get("code"))).Scan(&route, &challenge, &redirect, &cid, &redeemed)
		sum := sha256.Sum256([]byte(f.Get("code_verifier")))
		proof := base64.RawURLEncoding.EncodeToString(sum[:])
		if e != nil || redeemed || cid != f.Get("client_id") || redirect != f.Get("redirect_uri") || !mcpVerifier.MatchString(f.Get("code_verifier")) || subtle.ConstantTimeCompare([]byte(proof), []byte(challenge)) != 1 {
			failure(w, 400, "invalid_grant")
			return
		}
		_, e = tx.Exec(r.Context(), "UPDATE mcp_authorizations SET redeemed=true,code_sealed=NULL WHERE code_hash=$1", s.hash(f.Get("code")))
	} else if f.Get("grant_type") == "refresh_token" {
		var used bool
		var cid string
		e = tx.QueryRow(r.Context(), "SELECT t.route_id::text,t.used,r.client_id FROM mcp_tokens t JOIN mcp_routes r ON r.id=t.route_id WHERE refresh_hash=$1 AND refresh_expires>now() FOR UPDATE OF t", s.hash(f.Get("refresh_token"))).Scan(&route, &used, &cid)
		if e != nil || cid != f.Get("client_id") {
			failure(w, 400, "invalid_grant")
			return
		}
		if used {
			_, _ = tx.Exec(r.Context(), "UPDATE mcp_routes SET revoked=true WHERE id=$1", route)
			_ = tx.Commit(r.Context())
			failure(w, 400, "invalid_grant")
			return
		}
		_, e = tx.Exec(r.Context(), "UPDATE mcp_tokens SET used=true WHERE refresh_hash=$1", s.hash(f.Get("refresh_token")))
	} else {
		failure(w, 400, "unsupported_grant_type")
		return
	}
	var scopes []string
	var live bool
	if e != nil || tx.QueryRow(r.Context(), "SELECT r.scopes,NOT r.revoked AND r.expires_at>now() AND d.status='approved' AND NOT u.disabled FROM mcp_routes r JOIN devices d ON d.account_id=r.account_id AND d.id=r.device_id JOIN users u ON u.id=r.account_id WHERE r.id=$1 FOR UPDATE OF r", route).Scan(&scopes, &live) != nil || !live {
		failure(w, 400, "invalid_grant")
		return
	}
	access, refresh := token(), token()
	_, e = tx.Exec(r.Context(), "INSERT INTO mcp_tokens(access_hash,refresh_hash,route_id) VALUES($1,$2,$3)", s.hash(access), s.hash(refresh), route)
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"access_token": access, "refresh_token": refresh, "token_type": "Bearer", "expires_in": 900, "scope": strings.Join(scopes, " ")})
}
func (s *Server) mcpRevoke(w http.ResponseWriter, r *http.Request) {
	r.Body = http.MaxBytesReader(w, r.Body, 4096)
	if r.ParseForm() != nil {
		failure(w, 400, "invalid_request")
		return
	}
	_, e := s.db.Exec(r.Context(), "UPDATE mcp_routes SET revoked=true WHERE client_id=$1 AND id IN(SELECT route_id FROM mcp_tokens WHERE access_hash=$2 OR refresh_hash=$2)", r.PostForm.Get("client_id"), s.hash(r.PostForm.Get("token")))
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	w.WriteHeader(200)
}
func (s *Server) mcpCipher() (cipher.AEAD, error) {
	key, _ := hex.DecodeString(s.hash("nodecloak-mcp-relay-encryption-v1"))
	b, e := aes.NewCipher(key)
	if e != nil {
		return nil, e
	}
	return cipher.NewGCM(b)
}
func (s *Server) mcpSeal(binding, value string) (string, error) {
	a, e := s.mcpCipher()
	if e != nil {
		return "", e
	}
	n := make([]byte, a.NonceSize())
	if _, e = rand.Read(n); e != nil {
		return "", e
	}
	return base64.RawStdEncoding.EncodeToString(a.Seal(n, n, []byte(value), []byte("mcp-v1:"+binding))), nil
}
func (s *Server) mcpOpen(binding, value string) (string, error) {
	a, e := s.mcpCipher()
	if e != nil {
		return "", e
	}
	b, e := base64.RawStdEncoding.DecodeString(value)
	if e != nil || len(b) < a.NonceSize()+a.Overhead() {
		return "", fmt.Errorf("invalid ciphertext")
	}
	p, e := a.Open(nil, b[:a.NonceSize()], b[a.NonceSize():], []byte("mcp-v1:"+binding))
	return string(p), e
}

var mcpConsent = template.Must(template.New("mcp-consent").Parse(`<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Connect NodeCloak</title><style nonce="{{.Nonce}}">:root{color-scheme:dark}*{box-sizing:border-box}body{margin:0;min-height:100vh;display:grid;place-items:center;background:#10151b;color:#f7f5f0;font:16px system-ui}main{width:min(92vw,520px);padding:40px;border:1px solid #29313a;border-radius:24px;background:#171e25}svg{width:56px;height:56px;color:#70dca0}h1{font-size:28px;line-height:1.3}p{line-height:1.7;color:#a5afbb}.code{font:28px ui-monospace,monospace;letter-spacing:5px;color:#70dca0;padding:20px 0}small{color:#a5afbb}#state{min-height:24px}</style><main><svg viewBox="0 0 64 64" aria-hidden="true"><path fill="currentColor" d="M12 55V29a20 20 0 0140 0v26l-7-6-7 6-6-6-6 6-7-6z"/><path stroke="#10151b" stroke-width="4" fill="none" d="M22 29h20"/><circle cx="22" cy="29" r="5" fill="#10151b"/><circle cx="42" cy="29" r="5" fill="#10151b"/></svg><h1>Connect {{.Name}}</h1><p>Open NodeCloak → MCP → Remote access. Enter this code and choose the browser profiles and permissions for this connection.</p><div class="code">{{.Code}}</div><p>Authorized webpage content and screenshots pass through the NodeCloak gateway to this AI client.</p><small>Keep this page open. This request expires in 10 minutes.</small><p id="state" role="status">Waiting for approval in NodeCloak…</p></main><script nonce="{{.Nonce}}">const poll={{.Poll}};let attempts=0;const timer=setInterval(async()=>{if(++attempts>300){clearInterval(timer);document.getElementById('state').textContent='Request expired. Start a new connection.';return;}try{const r=await fetch('/mcp/oauth/pending/'+encodeURIComponent(poll));const v=await r.json();if(v.redirect){clearInterval(timer);location.replace(v.redirect);}else if(!r.ok){clearInterval(timer);document.getElementById('state').textContent='Request expired. Start a new connection.';}}catch{}},2000);</script></html>`))

func renderMCPConsent(w http.ResponseWriter, name, code, poll string) {
	nonce := token()
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Content-Security-Policy", "default-src 'none'; script-src 'nonce-"+nonce+"'; style-src 'nonce-"+nonce+"'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'")
	_ = mcpConsent.Execute(w, map[string]string{"Name": name, "Code": code, "Poll": poll, "Nonce": nonce})
}
