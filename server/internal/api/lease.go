package api

import "net/http"

// Retain the fence row: the next acquisition must advance its generation.
// A delayed close or an old owner can never release a newer browser launch.
func (s *Server) releaseLease(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		Generation int64 `json:"generation"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	id := r.PathValue("id")
	if !validID(id) || in.Generation <= 0 {
		failure(w, 400, "invalid_request")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	if s.authorizeTx(r.Context(), tx, i, true) != nil {
		failure(w, 401, "unauthorized")
		return
	}
	if _, e = tx.Exec(r.Context(), "SELECT id FROM users WHERE id=$1 FOR UPDATE", i.Account); e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	result, e := tx.Exec(r.Context(), "UPDATE leases SET expires_at=now() WHERE account_id=$1 AND object_id=$2 AND owner=$3 AND generation=$4", i.Account, id, i.Device, in.Generation)
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"released": result.RowsAffected() == 1})
}
