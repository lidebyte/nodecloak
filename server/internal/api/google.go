package api

import (
	"encoding/json"
	"github.com/coreos/go-oidc/v3/oidc"
	"golang.org/x/oauth2"
	"golang.org/x/oauth2/google"
	"net/http"
	"strings"
)

func (s *Server) googleConfig() oauth2.Config {
	return oauth2.Config{ClientID: s.cfg.GoogleClientID, ClientSecret: s.cfg.GoogleClientSecret, RedirectURL: strings.TrimRight(s.cfg.PublicURL, "/") + "/v1/auth/google/callback", Endpoint: google.Endpoint, Scopes: []string{"openid", "email", "profile"}}
}
func (s *Server) googleStart(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Device deviceInput `json:"device"`
	}
	if !decode(w, r, &in, 8192) {
		return
	}
	if !in.Device.valid() {
		failure(w, 400, "invalid_device")
		return
	}
	if s.cfg.GoogleClientID == "" || s.cfg.GoogleClientSecret == "" {
		failure(w, 503, "google_not_configured")
		return
	}
	if !s.limit(r.Context(), "google:"+s.clientIP(r), 10) {
		failure(w, 429, "try_later")
		return
	}
	state, poll, nonce, verifier := token(), token(), token(), oauth2.GenerateVerifier()
	in.Device.Secret = s.hash("device:" + in.Device.Secret)
	device, _ := json.Marshal(in.Device)
	_, e := s.db.Exec(r.Context(), `INSERT INTO oauth_flows(state_hash,poll_hash,nonce,verifier,device,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '10 minutes')`, s.hash(state), s.hash(poll), nonce, verifier, device)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	cfg := s.googleConfig()
	reply(w, 200, map[string]string{"url": cfg.AuthCodeURL(state, oidc.Nonce(nonce), oauth2.S256ChallengeOption(verifier)), "pollToken": poll})
}
func (s *Server) googleCallback(w http.ResponseWriter, r *http.Request) {
	state := r.URL.Query().Get("state")
	var nonce, verifier string
	e := s.db.QueryRow(r.Context(), `UPDATE oauth_flows SET callback_used=true WHERE state_hash=$1 AND NOT callback_used AND expires_at>now() RETURNING nonce,verifier`, s.hash(state)).Scan(&nonce, &verifier)
	if e != nil {
		renderOAuthPage(w, r, 400, "invalid_oauth_state")
		return
	}
	finish := func(v any) {
		b, _ := json.Marshal(v)
		_, _ = s.db.Exec(r.Context(), "UPDATE oauth_flows SET result=$2 WHERE state_hash=$1", s.hash(state), b)
	}
	if r.URL.Query().Get("error") != "" {
		finish(map[string]string{"error": "google_cancelled"})
		renderOAuthPage(w, r, 400, "google_cancelled")
		return
	}
	cfg := s.googleConfig()
	ctx := oidc.ClientContext(r.Context(), s.http)
	auth, e := cfg.Exchange(ctx, r.URL.Query().Get("code"), oauth2.VerifierOption(verifier))
	if e != nil {
		finish(map[string]string{"error": "google_exchange_failed"})
		renderOAuthPage(w, r, 401, "google_exchange_failed")
		return
	}
	raw, ok := auth.Extra("id_token").(string)
	if !ok {
		finish(map[string]string{"error": "google_verification_failed"})
		renderOAuthPage(w, r, 401, "google_verification_failed")
		return
	}
	keys := oidc.NewRemoteKeySet(ctx, "https://www.googleapis.com/oauth2/v3/certs")
	verified, e := oidc.NewVerifier("https://accounts.google.com", keys, &oidc.Config{ClientID: s.cfg.GoogleClientID, SupportedSigningAlgs: []string{oidc.RS256}}).Verify(ctx, raw)
	var claims struct {
		Email    string `json:"email"`
		Verified bool   `json:"email_verified"`
		Nonce    string `json:"nonce"`
	}
	if e != nil || verified.Claims(&claims) != nil || claims.Nonce != nonce || !claims.Verified || verified.Subject == "" {
		finish(map[string]string{"error": "google_verification_failed"})
		renderOAuthPage(w, r, 401, "google_verification_failed")
		return
	}
	email, valid := normalizedEmail(claims.Email)
	if !valid {
		finish(map[string]string{"error": "google_verification_failed"})
		renderOAuthPage(w, r, 401, "google_verification_failed")
		return
	}
	finish(map[string]string{"email": email, "sub": verified.Subject})
	renderOAuthPage(w, r, http.StatusOK, "")
}
func (s *Server) googlePoll(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Token string `json:"pollToken"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var rawDevice, rawResult []byte
	e = tx.QueryRow(r.Context(), "SELECT device,result FROM oauth_flows WHERE poll_hash=$1 AND NOT consumed AND expires_at>now() FOR UPDATE", s.hash(in.Token)).Scan(&rawDevice, &rawResult)
	if e != nil {
		failure(w, 401, "invalid_oauth_flow")
		return
	}
	if len(rawResult) == 0 {
		reply(w, 202, map[string]bool{"pending": true})
		return
	}
	var result map[string]string
	var d deviceInput
	if json.Unmarshal(rawResult, &result) != nil || json.Unmarshal(rawDevice, &d) != nil {
		failure(w, 503, "invalid_oauth_flow")
		return
	}
	_, e = tx.Exec(r.Context(), "UPDATE oauth_flows SET consumed=true WHERE poll_hash=$1", s.hash(in.Token))
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if result["error"] != "" {
		failure(w, 401, result["error"])
		return
	}
	// Flow stores only the device authenticator hash, never its raw secret or session tokens.
	if r.Header.Get("Authorization") != "" {
		s.auth(false, func(w http.ResponseWriter, r *http.Request, i identity) {
			tx, e := s.db.Begin(r.Context())
			if e != nil {
				failure(w, 503, "database_unavailable")
				return
			}
			defer tx.Rollback(r.Context())
			if s.authorizeTx(r.Context(), tx, i, false) != nil {
				failure(w, 401, "unauthorized")
				return
			}
			tag, e := tx.Exec(r.Context(), `UPDATE users SET google_sub=$3 WHERE id=$1 AND email=$2 AND (google_sub IS NULL OR google_sub=$3)`, i.Account, result["email"], result["sub"])
			if e != nil || tag.RowsAffected() != 1 {
				failure(w, 409, "account_link_conflict")
				return
			}
			_, e = tx.Exec(r.Context(), "UPDATE sessions SET auth_at=now(),auth_method='google' WHERE id=$1", i.Session)
			if e == nil {
				e = tx.Commit(r.Context())
			}
			if e != nil {
				failure(w, 503, "database_unavailable")
				return
			}
			s.audit(r.Context(), i, "google_linked", "")
			reply(w, 200, map[string]bool{"linked": true})
		})(w, r)
		return
	}
	s.completeLoginWithHash(w, r, result["email"], result["sub"], d, d.Secret)
}
