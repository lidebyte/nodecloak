package api

import (
	"context"
	"crypto/rand"
	"crypto/subtle"
	"fmt"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"math/big"
	"net/http"
	"net/url"
	"time"
)

func BootstrapAdmins(ctx context.Context, db *pgxpool.Pool, emails []string) error {
	tx, e := db.Begin(ctx)
	if e != nil {
		return e
	}
	defer tx.Rollback(ctx)
	if _, e = tx.Exec(ctx, "SELECT pg_advisory_xact_lock(748913227)"); e != nil {
		return e
	}
	for _, raw := range emails {
		email, valid := normalizedEmail(raw)
		if !valid {
			return fmt.Errorf("invalid administrator email")
		}
		var id string
		e = tx.QueryRow(ctx, "SELECT id::text FROM users WHERE email=$1 AND NOT disabled FOR UPDATE", email).Scan(&id)
		if e == pgx.ErrNoRows {
			continue
		}
		if e != nil {
			return e
		}
		tag, e := tx.Exec(ctx, "INSERT INTO admin_bootstrap(email) VALUES($1) ON CONFLICT DO NOTHING", email)
		if e != nil {
			return e
		}
		if tag.RowsAffected() == 1 {
			if _, e = tx.Exec(ctx, "UPDATE users SET role='admin' WHERE id=$1", id); e != nil {
				return e
			}
			if _, e = tx.Exec(ctx, "INSERT INTO admin_audit(actor_id,actor_email,event,target_id,detail) VALUES($1::uuid,$2,'admin.bootstrap',($1::uuid)::text,'{}')", id, email); e != nil {
				return e
			}

		}
	}
	return tx.Commit(ctx)
}

type adminIdentity struct {
	ID    string `json:"id"`
	Email string `json:"email"`
	Name  string `json:"name"`
	Role  string `json:"role"`
	CSRF  string `json:"-"`
}
type adminHandler func(http.ResponseWriter, *http.Request, adminIdentity)

const adminCookie = "nodecloak_admin"

func (s *Server) adminOrigin(r *http.Request) bool {
	u, e := url.Parse(s.cfg.PublicURL)
	return e == nil && r.Header.Get("Origin") == u.Scheme+"://"+u.Host
}
func (s *Server) adminAuth(next adminHandler) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		c, e := r.Cookie(adminCookie)
		if e != nil || len(c.Value) != 43 {
			failure(w, 401, "unauthorized")
			return
		}
		var a adminIdentity
		e = s.db.QueryRow(r.Context(), `SELECT u.id::text,u.email,u.display_name,u.role,a.csrf FROM admin_sessions a JOIN users u ON u.id=a.account_id WHERE a.token_hash=$1 AND a.expires_at>now() AND u.role='admin' AND NOT u.disabled`, s.hash("admin:"+c.Value)).Scan(&a.ID, &a.Email, &a.Name, &a.Role, &a.CSRF)
		if e != nil {
			failure(w, 401, "unauthorized")
			return
		}
		if r.Method != "GET" && (subtle.ConstantTimeCompare([]byte(r.Header.Get("X-CSRF-Token")), []byte(a.CSRF)) != 1 || (r.Header.Get("Origin") != "" && !s.adminOrigin(r))) {
			failure(w, 403, "csrf_denied")
			return
		}
		next(w, r, a)
	}
}
func (s *Server) registerAdmin() {
	s.mux.HandleFunc("GET /admin/", s.adminAssets)
	s.mux.HandleFunc("POST /v1/admin/email/request", s.adminEmail)
	s.mux.HandleFunc("POST /v1/admin/login", s.adminLogin)
	s.mux.HandleFunc("GET /v1/admin/session", s.adminAuth(func(w http.ResponseWriter, r *http.Request, a adminIdentity) {
		reply(w, 200, map[string]any{"user": a, "csrf": a.CSRF})
	}))
	s.mux.HandleFunc("POST /v1/admin/logout", s.adminAuth(s.adminLogout))
	s.mux.HandleFunc("GET /v1/admin/summary", s.adminAuth(s.adminSummary))
	s.mux.HandleFunc("GET /v1/admin/catalog", s.adminAuth(s.adminCatalogGet))
	s.mux.HandleFunc("PUT /v1/admin/catalog", s.adminAuth(s.adminCatalogPut))
	s.mux.HandleFunc("POST /v1/admin/catalog/publish", s.adminAuth(s.adminPublish))
	s.mux.HandleFunc("GET /v1/catalog", s.publicCatalog)
	s.mux.HandleFunc("GET /v1/catalog/icons/{name}", s.catalogIcon)
	s.mux.HandleFunc("POST /v1/admin/icons", s.adminAuth(s.adminIconUpload))
	s.mux.HandleFunc("GET /v1/admin/users", s.adminAuth(s.adminUsers))
	s.mux.HandleFunc("PATCH /v1/admin/users/{id}", s.adminAuth(s.adminUserPatch))
	s.mux.HandleFunc("POST /v1/admin/users/{id}/sessions/revoke", s.adminAuth(s.adminUserRevoke))
	s.mux.HandleFunc("GET /v1/admin/devices", s.adminAuth(s.adminDevices))
	s.mux.HandleFunc("POST /v1/admin/devices/{accountId}/{id}/sessions/revoke", s.adminAuth(s.adminDeviceRevoke))
	s.mux.HandleFunc("GET /v1/admin/cloud", s.adminAuth(s.adminCloud))
	s.mux.HandleFunc("GET /v1/admin/audit", s.adminAuth(s.adminAuditList))
	s.mux.HandleFunc("GET /v1/admin/system", s.adminAuth(s.adminSystemGet))
	s.mux.HandleFunc("PUT /v1/admin/system", s.adminAuth(s.adminSystemPut))
	s.mux.HandleFunc("GET /v1/admin/announcements", s.adminAuth(s.adminAnnouncements))
	s.mux.HandleFunc("POST /v1/admin/announcements", s.adminAuth(s.adminAnnouncementWrite))
	s.mux.HandleFunc("PATCH /v1/admin/announcements/{id}", s.adminAuth(s.adminAnnouncementWrite))
	s.mux.HandleFunc("DELETE /v1/admin/announcements/{id}", s.adminAuth(s.adminAnnouncementDelete))
}
func (s *Server) adminEmail(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Email string `json:"email"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	email, ok := normalizedEmail(in.Email)
	if !ok {
		failure(w, 400, "invalid_email")
		return
	}
	if !s.limit(r.Context(), "admin-email:"+email, 2) || !s.limit(r.Context(), "admin-mail-ip:"+s.clientIP(r), 10) {
		failure(w, 429, "try_later")
		return
	}
	var eligible bool
	if s.db.QueryRow(r.Context(), "SELECT EXISTS(SELECT 1 FROM users WHERE email=$1 AND role='admin' AND NOT disabled)", email).Scan(&eligible) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if !eligible {
		reply(w, 202, map[string]bool{"sent": true})
		return
	}
	if s.mail == nil {
		failure(w, 503, "email_not_configured")
		return
	}
	n, e := rand.Int(rand.Reader, big.NewInt(1000000))
	if e != nil {
		failure(w, 503, "entropy_unavailable")
		return
	}
	code := fmt.Sprintf("%06d", n.Int64())
	hash := s.hash("admin-otp:" + email + ":" + code)
	if _, e = s.db.Exec(r.Context(), `INSERT INTO admin_email_codes(email,code_hash,expires_at) VALUES($1,$2,now()+interval '10 minutes') ON CONFLICT(email) DO UPDATE SET code_hash=$2,expires_at=now()+interval '10 minutes',attempts=0`, email, hash); e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if s.mail.Send(r.Context(), email, code) != nil {
		_, _ = s.db.Exec(r.Context(), "DELETE FROM admin_email_codes WHERE email=$1 AND code_hash=$2", email, hash)
		failure(w, 503, "email_delivery_failed")
		return
	}
	reply(w, 202, map[string]bool{"sent": true})
}
func (s *Server) adminLogin(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Email     string `json:"email"`
		Password  string `json:"password"`
		EmailCode string `json:"emailCode"`
		Factor    string `json:"factor"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	email, ok := normalizedEmail(in.Email)
	if !ok || len(in.Password) > 512 || len(in.Factor) > 64 || (in.Password == "") == (in.EmailCode == "") {
		failure(w, 401, "invalid_credentials")
		return
	}
	if !s.limit(r.Context(), "admin-login:"+email, 5) || !s.limit(r.Context(), "admin-login-ip:"+s.clientIP(r), 20) {
		failure(w, 429, "try_later")
		return
	}
	release, ok := takePasswordBudget(w)
	if !ok {
		return
	}
	defer release()
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var a adminIdentity
	var hash, totp string
	e = tx.QueryRow(r.Context(), "SELECT id::text,email,display_name,role,password_hash,totp_secret FROM users WHERE email=$1 AND role='admin' AND NOT disabled FOR UPDATE", email).Scan(&a.ID, &a.Email, &a.Name, &a.Role, &hash, &totp)
	if e != nil {
		if in.Password != "" {
			passwordKey(in.Password, make([]byte, 16))
		}
		failure(w, 401, "invalid_credentials")
		return
	}
	if in.Password != "" {
		if hash == "" || !checkPassword(hash, in.Password) {
			failure(w, 401, "invalid_credentials")
			return
		}
	} else {
		var expected string
		var attempts int
		var live bool
		if tx.QueryRow(r.Context(), "SELECT code_hash,attempts,expires_at>now() FROM admin_email_codes WHERE email=$1 FOR UPDATE", email).Scan(&expected, &attempts, &live) != nil || !live || attempts >= 5 {
			failure(w, 401, "invalid_credentials")
			return
		}
		if subtle.ConstantTimeCompare([]byte(expected), []byte(s.hash("admin-otp:"+email+":"+in.EmailCode))) != 1 {
			_, _ = tx.Exec(r.Context(), "UPDATE admin_email_codes SET attempts=attempts+1 WHERE email=$1", email)
			_ = tx.Commit(r.Context())
			failure(w, 401, "invalid_credentials")
			return
		}
	}
	if totp != "" && !s.acceptFactor(r.Context(), tx, a.ID, in.Factor, true) {
		if in.EmailCode != "" {
			_, _ = tx.Exec(r.Context(), "UPDATE admin_email_codes SET attempts=attempts+1 WHERE email=$1", email)
			_ = tx.Commit(r.Context())
		}
		failure(w, 401, "factor_required")
		return
	}
	if in.EmailCode != "" {
		if _, e = tx.Exec(r.Context(), "DELETE FROM admin_email_codes WHERE email=$1", email); e != nil {
			failure(w, 503, "database_unavailable")
			return
		}
	}
	secret := token()
	a.CSRF = token()
	_, e = tx.Exec(r.Context(), "INSERT INTO admin_sessions(token_hash,account_id,csrf,expires_at) VALUES($1,$2,$3,now()+interval '8 hours')", s.hash("admin:"+secret), a.ID, a.CSRF)
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "admin.login", a.ID, map[string]any{})
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	u, _ := url.Parse(s.cfg.PublicURL)
	http.SetCookie(w, &http.Cookie{Name: adminCookie, Value: secret, Path: "/v1/admin/", Secure: u.Scheme == "https", HttpOnly: true, SameSite: http.SameSiteStrictMode, MaxAge: 8 * 3600, Expires: s.now().Add(8 * time.Hour)})
	reply(w, 200, map[string]any{"user": a, "csrf": a.CSRF})
}
func (s *Server) adminLogout(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	c, _ := r.Cookie(adminCookie)
	if _, e := s.db.Exec(r.Context(), "DELETE FROM admin_sessions WHERE token_hash=$1", s.hash("admin:"+c.Value)); e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	http.SetCookie(w, &http.Cookie{Name: adminCookie, Path: "/v1/admin/", HttpOnly: true, SameSite: http.SameSiteStrictMode, MaxAge: -1})
	reply(w, 200, map[string]bool{"ok": true})
}
func (s *Server) adminAuditTx(ctx context.Context, tx pgx.Tx, a adminIdentity, event, target string, detail any) error {
	_, e := tx.Exec(ctx, "INSERT INTO admin_audit(actor_id,actor_email,event,target_id,detail) VALUES($1,$2,$3,$4,$5)", a.ID, a.Email, event, target, detail)
	return e
}
