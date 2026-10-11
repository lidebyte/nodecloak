package api

import (
	"context"
	"github.com/jackc/pgx/v5"
	"net/http"
	"strings"
	"time"
	"unicode/utf8"
)

type announcement struct {
	ID          string    `json:"id"`
	Title       string    `json:"title"`
	Body        string    `json:"body"`
	Revision    int64     `json:"revision"`
	Published   bool      `json:"published"`
	CreatedAt   time.Time `json:"createdAt"`
	UpdatedAt   time.Time `json:"updatedAt"`
	PublishedAt time.Time `json:"publishedAt"`
}

const announcementColumns = "id::text,title,body,revision,published,created_at,updated_at,published_at"

func scanAnnouncement(row pgx.Row) (announcement, error) {
	var a announcement
	e := row.Scan(&a.ID, &a.Title, &a.Body, &a.Revision, &a.Published, &a.CreatedAt, &a.UpdatedAt, &a.PublishedAt)
	return a, e
}
func (s *Server) announcements(ctx context.Context, public bool) ([]announcement, error) {
	filter := "NOT deleted"
	if public {
		filter += " AND published"
	}
	rows, e := s.db.Query(ctx, "SELECT "+announcementColumns+" FROM admin_announcements WHERE "+filter+" ORDER BY published_at DESC,created_at DESC,id DESC LIMIT 100")
	if e != nil {
		return nil, e
	}
	defer rows.Close()
	result := []announcement{}
	for rows.Next() {
		a, e := scanAnnouncement(rows)
		if e != nil {
			return nil, e
		}
		result = append(result, a)
	}
	return result, rows.Err()
}
func (s *Server) adminAnnouncements(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	s.adminList(w, r,
		"SELECT count(*) FROM admin_announcements WHERE NOT deleted AND (title ILIKE $1 OR body ILIKE $1)",
		`SELECT jsonb_build_object('id',id,'title',title,'body',body,'revision',revision,'published',published,'createdAt',created_at,'updatedAt',updated_at,'publishedAt',published_at)
		FROM admin_announcements WHERE NOT deleted AND (title ILIKE $1 OR body ILIKE $1)
		ORDER BY published_at DESC,created_at DESC,id DESC LIMIT $2 OFFSET $3`)
}
func (s *Server) adminAnnouncementWrite(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		Title     string `json:"title"`
		Body      string `json:"body"`
		Published bool   `json:"published"`
		Revision  int64  `json:"revision"`
	}
	if !decode(w, r, &in, 16384) {
		return
	}
	in.Title = strings.TrimSpace(in.Title)
	in.Body = strings.TrimSpace(in.Body)
	if in.Title == "" || in.Body == "" || !utf8.ValidString(in.Title) || !utf8.ValidString(in.Body) || utf8.RuneCountInString(in.Title) > 160 || utf8.RuneCountInString(in.Body) > 2000 {
		failure(w, 400, "invalid_announcement")
		return
	}
	create := r.Method == "POST"
	id := r.PathValue("id")
	if create {
		id = uuid()
	} else if !validID(id) || in.Revision < 1 {
		failure(w, 400, "invalid_announcement")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var saved announcement
	if create {
		// Serialize the capacity check and creation; the public catalog remains bounded.
		if _, e = tx.Exec(r.Context(), "SELECT pg_advisory_xact_lock(748913228)"); e == nil {
			var count int
			e = tx.QueryRow(r.Context(), "SELECT count(*) FROM admin_announcements WHERE NOT deleted").Scan(&count)
			if e == nil && count >= 100 {
				failure(w, 409, "announcement_limit")
				return
			}
		}
		if e == nil {
			saved, e = scanAnnouncement(tx.QueryRow(r.Context(), "INSERT INTO admin_announcements(id,title,body,published) VALUES($1,$2,$3,$4) RETURNING "+announcementColumns, id, in.Title, in.Body, in.Published))
		}
	} else {
		saved, e = scanAnnouncement(tx.QueryRow(r.Context(), "UPDATE admin_announcements SET title=$2,body=$3,published=$4,published_at=CASE WHEN $4 AND NOT published THEN clock_timestamp() ELSE published_at END,updated_at=clock_timestamp(),revision=revision+1 WHERE id=$1 AND revision=$5 AND NOT deleted RETURNING "+announcementColumns, id, in.Title, in.Body, in.Published, in.Revision))
	}
	if e == pgx.ErrNoRows {
		failure(w, 409, "revision_conflict")
		return
	}
	event := "announcement.update"
	if create {
		event = "announcement.create"
	}
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, event, id, map[string]any{"revision": saved.Revision, "published": saved.Published})
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	status := 200
	if create {
		status = 201
	}
	reply(w, status, saved)
}
func (s *Server) adminAnnouncementDelete(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		Revision int64 `json:"revision"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	id := r.PathValue("id")
	if !validID(id) || in.Revision < 1 {
		failure(w, 400, "invalid_announcement")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	tag, e := tx.Exec(r.Context(), "UPDATE admin_announcements SET deleted=true,published=false,updated_at=clock_timestamp(),revision=revision+1 WHERE id=$1 AND revision=$2 AND NOT deleted", id, in.Revision)
	if e == nil && tag.RowsAffected() == 0 {
		failure(w, 409, "revision_conflict")
		return
	}
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "announcement.archive", id, map[string]any{"revision": in.Revision + 1})
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
