package api

import (
	"encoding/json"
	"net/http"
	"strconv"
	"strings"
)

func adminPage(r *http.Request) (string, int, int) {
	q := strings.TrimSpace(r.URL.Query().Get("q"))
	if len(q) > 200 {
		q = q[:200]
	}
	p, _ := strconv.Atoi(r.URL.Query().Get("page"))
	n, _ := strconv.Atoi(r.URL.Query().Get("pageSize"))
	if p < 1 {
		p = 1
	}
	if p > 100000 {
		p = 100000
	}
	if n < 1 || n > 100 {
		n = 20
	}
	return "%" + q + "%", p, n
}
func (s *Server) adminList(w http.ResponseWriter, r *http.Request, count, query string) {
	q, p, n := adminPage(r)
	var total int64
	if s.db.QueryRow(r.Context(), count, q).Scan(&total) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	rows, e := s.db.Query(r.Context(), query, q, n, (p-1)*n)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer rows.Close()
	items := []json.RawMessage{}
	for rows.Next() {
		var raw []byte
		if rows.Scan(&raw) != nil {
			failure(w, 503, "database_unavailable")
			return
		}
		items = append(items, json.RawMessage(raw))
	}
	if rows.Err() != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"items": items, "total": total, "page": p, "pageSize": n})
}
func (s *Server) adminSummary(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var raw []byte
	e := s.db.QueryRow(r.Context(), `SELECT jsonb_build_object('users',(SELECT count(*) FROM users),'devices',(SELECT count(*) FROM devices WHERE status<>'revoked'),'activeSessions',(SELECT count(*) FROM sessions s JOIN users u ON u.id=s.account_id WHERE NOT s.revoked AND s.refresh_expires>now() AND NOT u.disabled),'cloudObjects',(SELECT count(*) FROM objects),'cloudBytes',(SELECT COALESCE(sum(octet_length(payload)),0) FROM objects),'publishedRevision',(SELECT published_revision FROM admin_catalog),'auditEvents',(SELECT count(*) FROM audit)+(SELECT count(*) FROM admin_audit))`).Scan(&raw)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, json.RawMessage(raw))
}
func (s *Server) adminUsers(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminList(w, r, `SELECT count(*) FROM users WHERE email ILIKE $1 OR display_name ILIKE $1`, `SELECT jsonb_build_object('id',u.id,'email',u.email,'name',u.display_name,'role',u.role,'disabled',u.disabled,'createdAt',u.created_at,'googleLinked',u.google_sub IS NOT NULL,'hasPassword',u.password_hash<>'','twoFactorEnabled',u.totp_secret<>'','devices',(SELECT count(*) FROM devices d WHERE d.account_id=u.id AND d.status<>'revoked'),'cloudBytes',(SELECT COALESCE(sum(octet_length(o.payload)),0) FROM objects o WHERE o.account_id=u.id)) FROM users u WHERE email ILIKE $1 OR display_name ILIKE $1 ORDER BY created_at DESC,id LIMIT $2 OFFSET $3`)
}
func (s *Server) adminDevices(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminList(w, r, `SELECT count(*) FROM devices d JOIN users u ON u.id=d.account_id WHERE u.email ILIKE $1 OR d.name ILIKE $1`, `SELECT jsonb_build_object('id',d.id,'accountId',d.account_id,'email',u.email,'name',d.name,'status',d.status,'epoch',d.epoch,'createdAt',d.created_at,'activeSessions',(SELECT count(*) FROM sessions s WHERE s.account_id=d.account_id AND s.device_id=d.id AND NOT s.revoked AND s.refresh_expires>now())) FROM devices d JOIN users u ON u.id=d.account_id WHERE u.email ILIKE $1 OR d.name ILIKE $1 ORDER BY d.created_at DESC,d.account_id,d.id LIMIT $2 OFFSET $3`)
}
func (s *Server) adminCloud(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminList(w, r, `SELECT count(*) FROM objects o JOIN users u ON u.id=o.account_id WHERE u.email ILIKE $1 OR o.id::text ILIKE $1`, `SELECT jsonb_build_object('accountId',o.account_id,'email',u.email,'id',o.id,'kind',o.kind,'revision',o.revision,'epoch',o.epoch,'deleted',o.deleted,'bytes',octet_length(o.payload),'updatedAt',o.updated_at,'writer',o.writer) FROM objects o JOIN users u ON u.id=o.account_id WHERE u.email ILIKE $1 OR o.id::text ILIKE $1 ORDER BY o.updated_at DESC,o.account_id,o.id,o.kind LIMIT $2 OFFSET $3`)
}

const auditUnion = `(SELECT 'admin-'||id::text AS id,actor_email AS email,event,target_id AS target,detail,created_at FROM admin_audit UNION ALL SELECT 'user-'||a.id::text,u.email,a.event,COALESCE(a.object_id::text,a.device_id::text,''),jsonb_build_object('deviceId',a.device_id),a.created_at FROM audit a JOIN users u ON u.id=a.account_id) x`

func (s *Server) adminAuditList(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminList(w, r, `SELECT count(*) FROM `+auditUnion+` WHERE email ILIKE $1 OR event ILIKE $1 OR target ILIKE $1`, `SELECT jsonb_build_object('id',id,'actorEmail',email,'event',event,'targetId',target,'detail',detail,'createdAt',created_at) FROM `+auditUnion+` WHERE email ILIKE $1 OR event ILIKE $1 OR target ILIKE $1 ORDER BY created_at DESC,id LIMIT $2 OFFSET $3`)
}
func (s *Server) adminUserPatch(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	id := r.PathValue("id")
	if !validID(id) {
		failure(w, 400, "invalid_user")
		return
	}
	var in struct {
		Role     *string `json:"role"`
		Disabled *bool   `json:"disabled"`
		Name     *string `json:"name"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	if in.Role != nil && *in.Role != "user" && *in.Role != "admin" || in.Name != nil && (strings.TrimSpace(*in.Name) == "" || len(*in.Name) > 80) {
		failure(w, 400, "invalid_user")
		return
	}
	if id == a.ID && (in.Role != nil && *in.Role != "admin" || in.Disabled != nil && *in.Disabled) {
		failure(w, 409, "self_change")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	// Serialize role edits before any user row locks. This protects the final administrator across concurrent requests.
	if _, e = tx.Exec(r.Context(), "SELECT pg_advisory_xact_lock(748913227)"); e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	var role, name string
	var disabled bool
	if tx.QueryRow(r.Context(), "SELECT role,disabled,display_name FROM users WHERE id=$1 FOR UPDATE", id).Scan(&role, &disabled, &name) != nil {
		failure(w, 404, "user_not_found")
		return
	}
	if in.Role != nil {
		role = *in.Role
	}
	if in.Disabled != nil {
		disabled = *in.Disabled
	}
	if in.Name != nil {
		name = strings.TrimSpace(*in.Name)
	}
	if role != "admin" || disabled {
		var others int
		if tx.QueryRow(r.Context(), "SELECT count(*) FROM users WHERE role='admin' AND NOT disabled AND id<>$1", id).Scan(&others) != nil {
			failure(w, 503, "database_unavailable")
			return
		}
		if others == 0 {
			var wasAdmin bool
			_ = tx.QueryRow(r.Context(), "SELECT role='admin' AND NOT disabled FROM users WHERE id=$1", id).Scan(&wasAdmin)
			if wasAdmin {
				failure(w, 409, "last_admin")
				return
			}
		}
	}
	_, e = tx.Exec(r.Context(), "UPDATE users SET role=$2,disabled=$3,display_name=$4 WHERE id=$1", id, role, disabled, name)
	if e == nil && (disabled || role != "admin") {
		_, e = tx.Exec(r.Context(), "DELETE FROM admin_sessions WHERE account_id=$1", id)
	}
	if e == nil && disabled {
		_, e = tx.Exec(r.Context(), "UPDATE sessions SET revoked=true WHERE account_id=$1", id)
	}
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "user.update", id, map[string]any{"role": role, "disabled": disabled, "name": name})
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]bool{"ok": true})
}
func (s *Server) adminUserRevoke(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminRevoke(w, r, a, r.PathValue("id"), "")
}
func (s *Server) adminDeviceRevoke(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminRevoke(w, r, a, r.PathValue("accountId"), r.PathValue("id"))
}
func (s *Server) adminRevoke(w http.ResponseWriter, r *http.Request, a adminIdentity, account, device string) {
	if !validID(account) || device != "" && !validID(device) {
		failure(w, 400, "invalid_user")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var id string
	if tx.QueryRow(r.Context(), "SELECT id::text FROM users WHERE id=$1 FOR UPDATE", account).Scan(&id) != nil {
		failure(w, 404, "user_not_found")
		return
	}
	if device != "" {
		var exists bool
		if tx.QueryRow(r.Context(), "SELECT EXISTS(SELECT 1 FROM devices WHERE account_id=$1 AND id=$2)", account, device).Scan(&exists) != nil || !exists {
			failure(w, 404, "device_not_found")
			return
		}
	}
	_, e = tx.Exec(r.Context(), "UPDATE sessions SET revoked=true WHERE account_id=$1 AND ($2='' OR device_id::text=$2)", account, device)
	if e == nil && device == "" {
		_, e = tx.Exec(r.Context(), "DELETE FROM admin_sessions WHERE account_id=$1", account)
	}
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "sessions.revoke", account, map[string]any{"deviceId": device, "cryptographicApprovalUnchanged": true})
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]bool{"ok": true})
}

type adminSystem struct {
	SMTPConfigured      bool   `json:"smtpConfigured"`
	GoogleConfigured    bool   `json:"googleConfigured"`
	DatabaseReady       bool   `json:"databaseReady"`
	PublicURL           string `json:"publicURL"`
	RegistrationEnabled bool   `json:"registrationEnabled"`
	SupportURL          string `json:"supportURL"`
}

func (s *Server) system(r *http.Request) (adminSystem, error) {
	v := adminSystem{SMTPConfigured: s.mail != nil, GoogleConfigured: s.cfg.GoogleClientID != "" && s.cfg.GoogleClientSecret != "", DatabaseReady: s.db.Ping(r.Context()) == nil, PublicURL: s.cfg.PublicURL}
	e := s.db.QueryRow(r.Context(), "SELECT registration_enabled,support_url FROM admin_settings WHERE singleton=true").Scan(&v.RegistrationEnabled, &v.SupportURL)
	return v, e
}
func (s *Server) adminSystemGet(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	v, e := s.system(r)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, v)
}
func (s *Server) adminSystemPut(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		RegistrationEnabled bool   `json:"registrationEnabled"`
		SupportURL          string `json:"supportURL"`
	}
	if !decode(w, r, &in, 8192) {
		return
	}
	if in.SupportURL != "" && !safeDestination(in.SupportURL) {
		failure(w, 400, "invalid_settings")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	_, e = tx.Exec(r.Context(), "UPDATE admin_settings SET registration_enabled=$1,support_url=$2 WHERE singleton=true", in.RegistrationEnabled, in.SupportURL)
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "system.update", "system", in)
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.adminSystemGet(w, r, a)
}
