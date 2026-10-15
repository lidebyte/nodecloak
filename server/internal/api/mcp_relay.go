package api

import (
	"encoding/json"
	"net/http"
	"strings"
	"time"
)

func (s *Server) mcpRelay(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		Routes []string `json:"routes"`
	}
	if !decode(w, r, &in, 16384) {
		return
	}
	if len(in.Routes) > 100 {
		failure(w, 400, "invalid_request")
		return
	}
	for _, id := range in.Routes {
		if !validID(id) {
			failure(w, 400, "invalid_request")
			return
		}
	}
	s.mcpExpire(r)
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	rows, e := tx.Query(r.Context(), "UPDATE mcp_routes SET last_seen=now() WHERE id=ANY($1::uuid[]) AND account_id=$2 AND device_id=$3 AND NOT revoked AND expires_at>now() RETURNING id::text", in.Routes, i.Account, i.Device)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	active := []string{}
	for rows.Next() {
		var id string
		_ = rows.Scan(&id)
		active = append(active, id)
	}
	rows.Close()
	rows, e = tx.Query(r.Context(), `WITH q AS(SELECT j.id FROM mcp_jobs j JOIN mcp_routes r ON r.id=j.route_id WHERE j.route_id=ANY($1::uuid[]) AND r.account_id=$2 AND r.device_id=$3 AND NOT r.revoked AND r.expires_at>now() AND j.state='queued' AND NOT j.cancel_requested AND j.expires_at>now() ORDER BY j.created_at LIMIT 8 FOR UPDATE OF j SKIP LOCKED) UPDATE mcp_jobs j SET state='dispatched',dispatched_at=now(),updated_at=now() FROM q WHERE j.id=q.id RETURNING j.id::text,j.route_id::text,j.tool,j.arguments_sealed,j.expires_at`, active, i.Account, i.Device)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	jobs := []any{}
	for rows.Next() {
		var id, route, tool, sealed string
		var expires time.Time
		if rows.Scan(&id, &route, &tool, &sealed, &expires) != nil {
			continue
		}
		plain, err := s.mcpOpen("arguments:"+id, sealed)
		var args any
		if err != nil || json.Unmarshal([]byte(plain), &args) != nil {
			continue
		}
		jobs = append(jobs, map[string]any{"id": id, "route_id": route, "tool": tool, "arguments": args, "expires_at": expires.Unix()})
	}
	rows.Close()
	rows, e = tx.Query(r.Context(), "SELECT id::text,route_id::text FROM mcp_jobs WHERE route_id=ANY($1::uuid[]) AND cancel_requested AND state IN('dispatched','running','awaiting_approval')", active)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	cancels := []any{}
	for rows.Next() {
		var id, route string
		_ = rows.Scan(&id, &route)
		cancels = append(cancels, map[string]any{"id": id, "route_id": route})
	}
	rows.Close()
	if e = tx.Commit(r.Context()); e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"routes": active, "jobs": jobs, "cancellations": cancels})
}
func (s *Server) mcpUpdate(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		Route  string          `json:"route_id"`
		State  string          `json:"state"`
		Result json.RawMessage `json:"result"`
		Error  string          `json:"error"`
	}
	if !decode(w, r, &in, 12<<20) {
		return
	}
	if !validID(r.PathValue("id")) || !validID(in.Route) || !strings.Contains("|running|awaiting_approval|completed|failed|cancelled|", "|"+in.State+"|") || len(in.Error) > 200 {
		failure(w, 400, "invalid_request")
		return
	}
	if len(in.Result) == 0 {
		in.Result = json.RawMessage("null")
	}
	// Never persist browser error text, URLs or page text in an unencrypted field.
	if in.Error != "" {
		in.Error = strings.Split(in.Error, ":")[0]
		for _, c := range in.Error {
			if c != '_' && (c < 'A' || c > 'Z') {
				in.Error = "REMOTE_OPERATION_FAILED"
				break
			}
		}
	}
	sealed, e := s.mcpSeal("result:"+r.PathValue("id"), string(in.Result))
	if e != nil {
		failure(w, 503, "gateway_unavailable")
		return
	}
	tag, e := s.db.Exec(r.Context(), `UPDATE mcp_jobs j SET state=$4,result_sealed=$5,error=$6,arguments_sealed='',updated_at=now() FROM mcp_routes r WHERE j.id=$1 AND j.route_id=$2 AND r.id=j.route_id AND r.account_id=$3 AND r.device_id=$7 AND NOT r.revoked AND r.expires_at>now() AND j.expires_at>now() AND j.state IN('dispatched','running','awaiting_approval')`, r.PathValue("id"), in.Route, i.Account, in.State, sealed, in.Error, i.Device)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if tag.RowsAffected() != 1 {
		failure(w, 409, "job_not_active")
		return
	}
	reply(w, 200, map[string]any{"updated": true})
}
func (s *Server) mcpRouteRevoke(w http.ResponseWriter, r *http.Request, i identity) {
	if !validID(r.PathValue("id")) {
		failure(w, 400, "invalid_request")
		return
	}
	_, e := s.db.Exec(r.Context(), "UPDATE mcp_routes SET revoked=true WHERE id=$1 AND account_id=$2 AND device_id=$3", r.PathValue("id"), i.Account, i.Device)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, map[string]any{"revoked": true})
}
