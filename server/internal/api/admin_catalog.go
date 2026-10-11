package api

import (
	"bytes"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	_ "golang.org/x/image/webp"
	"image"
	_ "image/jpeg"
	"image/png"
	"net"
	"net/http"
	"net/url"
	"regexp"
	"sort"
	"strings"
)

type catalogSite struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	URL     string `json:"url"`
	Icon    string `json:"icon"`
	Order   int    `json:"order"`
	Enabled bool   `json:"enabled"`
}
type catalogGroup struct {
	ID      string        `json:"id"`
	Name    string        `json:"name"`
	NameEn  string        `json:"nameEn"`
	Icon    string        `json:"icon"`
	Order   int           `json:"order"`
	Enabled bool          `json:"enabled"`
	Sites   []catalogSite `json:"sites"`
}
type catalog struct {
	Revision          int64          `json:"revision"`
	Groups            []catalogGroup `json:"groups"`
	PublishedRevision int64          `json:"publishedRevision"`
}

var catalogID = regexp.MustCompile(`^[a-zA-Z0-9_-]{1,80}$`)
var builtinIcons = map[string]bool{"ai": true, "shopping": true, "claude": true, "chatgpt": true, "gemini": true, "grok": true, "muse": true, "amazon": true, "ebay": true, "etsy": true, "walmart": true, "shopee": true, "lazada": true, "globe": true}
var uploadedIcon = regexp.MustCompile(`^/v1/catalog/icons/[a-f0-9]{64}\.png$`)

func safeDestination(raw string) bool {
	u, e := url.Parse(raw)
	if e != nil || u.Scheme != "https" || u.Host == "" || u.User != nil || len(raw) > 2048 || (u.Port() != "" && u.Port() != "443") {
		return false
	}
	h := strings.ToLower(u.Hostname())
	for _, suffix := range []string{"localhost", "local", "internal", "lan", "home", "test", "invalid", "onion"} {
		if h == suffix || strings.HasSuffix(h, "."+suffix) {
			return false
		}
	}
	if net.ParseIP(h) != nil || !strings.Contains(h, ".") || strings.HasSuffix(h, ".") || h == "localhost" || strings.HasSuffix(h, ".localhost") || strings.HasSuffix(h, ".local") || strings.HasSuffix(h, ".internal") || strings.HasSuffix(h, ".lan") {
		return false
	}
	tld := h[strings.LastIndex(h, ".")+1:]
	if !regexp.MustCompile(`^[a-z][a-z0-9-]*[a-z]$`).MatchString(tld) {
		return false
	}
	for _, p := range strings.Split(h, ".") {
		if len(p) == 0 || len(p) > 63 || p[0] == '-' || p[len(p)-1] == '-' {
			return false
		}
		for _, c := range p {
			if !(c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '-') {
				return false
			}
		}
	}
	return true
}
func (s *Server) validateCatalog(r *http.Request, groups []catalogGroup) bool {
	if len(groups) > 40 {
		return false
	}
	ids := map[string]bool{}
	count := 0
	validID := func(v string) bool {
		if !catalogID.MatchString(v) || ids[v] {
			return false
		}
		ids[v] = true
		return true
	}
	validIcon := func(v string) bool {
		if builtinIcons[v] {
			return true
		}
		if !uploadedIcon.MatchString(v) {
			return false
		}
		var exists bool
		return s.db.QueryRow(r.Context(), "SELECT EXISTS(SELECT 1 FROM admin_icons WHERE name=$1)", strings.TrimPrefix(v, "/v1/catalog/icons/")).Scan(&exists) == nil && exists
	}
	for _, g := range groups {
		if !validID(g.ID) || strings.TrimSpace(g.Name) == "" || strings.TrimSpace(g.NameEn) == "" || len(g.Name) > 100 || len(g.NameEn) > 100 || g.Order < -100000 || g.Order > 100000 || !validIcon(g.Icon) || len(g.Sites) > 100 {
			return false
		}
		for _, x := range g.Sites {
			count++
			if count > 500 || !validID(x.ID) || strings.TrimSpace(x.Name) == "" || len(x.Name) > 100 || x.Order < -100000 || x.Order > 100000 || !safeDestination(x.URL) || !validIcon(x.Icon) {
				return false
			}
		}
	}
	return true
}
func (s *Server) readCatalog(r *http.Request) (catalog, error) {
	var c catalog
	c.Groups = []catalogGroup{}
	var raw []byte
	e := s.db.QueryRow(r.Context(), "SELECT revision,groups,published_revision FROM admin_catalog WHERE singleton=true").Scan(&c.Revision, &raw, &c.PublishedRevision)
	if e == nil {
		e = json.Unmarshal(raw, &c.Groups)
	}
	return c, e
}
func (s *Server) adminCatalogGet(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	c, e := s.readCatalog(r)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 200, c)
}
func (s *Server) adminCatalogPut(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		Revision int64          `json:"revision"`
		Groups   []catalogGroup `json:"groups"`
	}
	if !decode(w, r, &in, 512<<10) {
		return
	}
	if in.Revision < 1 || !s.validateCatalog(r, in.Groups) {
		failure(w, 400, "invalid_catalog")
		return
	}
	if in.Groups == nil {
		in.Groups = []catalogGroup{}
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	tag, e := tx.Exec(r.Context(), "UPDATE admin_catalog SET groups=$2,revision=revision+1 WHERE singleton=true AND revision=$1", in.Revision, in.Groups)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if tag.RowsAffected() != 1 {
		failure(w, 409, "revision_conflict")
		return
	}
	if e = s.adminAuditTx(r.Context(), tx, a, "catalog.save", "catalog", map[string]any{"revision": in.Revision + 1}); e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.adminCatalogGet(w, r, a)
}
func (s *Server) adminPublish(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		Revision int64 `json:"revision"`
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
	tag, e := tx.Exec(r.Context(), "UPDATE admin_catalog SET published_groups=groups,published_revision=revision WHERE singleton=true AND revision=$1", in.Revision)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if tag.RowsAffected() != 1 {
		failure(w, 409, "revision_conflict")
		return
	}
	if e = s.adminAuditTx(r.Context(), tx, a, "catalog.publish", "catalog", map[string]any{"revision": in.Revision}); e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.adminCatalogGet(w, r, a)
}
func (s *Server) publicCatalog(w http.ResponseWriter, r *http.Request) {
	var rev int64
	var raw []byte
	if s.db.QueryRow(r.Context(), "SELECT published_revision,published_groups FROM admin_catalog WHERE singleton=true").Scan(&rev, &raw) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	var groups []catalogGroup
	if json.Unmarshal(raw, &groups) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	result := []catalogGroup{}
	for _, g := range groups {
		if !g.Enabled {
			continue
		}
		sites := []catalogSite{}
		for _, x := range g.Sites {
			if x.Enabled {
				sites = append(sites, x)
			}
		}
		sort.SliceStable(sites, func(i, j int) bool { return sites[i].Order < sites[j].Order })
		g.Sites = sites
		result = append(result, g)
	}
	sort.SliceStable(result, func(i, j int) bool { return result[i].Order < result[j].Order })
	var support string
	if s.db.QueryRow(r.Context(), "SELECT support_url FROM admin_settings WHERE singleton=true").Scan(&support) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	list, e := s.announcements(r.Context(), true)
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	latest := ""
	if len(list) > 0 {
		latest = list[0].Body
	}
	reply(w, 200, map[string]any{"revision": rev, "groups": result, "supportURL": support, "announcement": latest, "announcements": list})
}
func (s *Server) adminIconUpload(w http.ResponseWriter, r *http.Request, a adminIdentity) {
	var in struct {
		Data string `json:"data"`
	}
	if !decode(w, r, &in, 3<<20) {
		return
	}
	parts := strings.SplitN(in.Data, ",", 2)
	if len(parts) != 2 || (parts[0] != "data:image/png;base64" && parts[0] != "data:image/jpeg;base64" && parts[0] != "data:image/webp;base64") {
		failure(w, 400, "invalid_image")
		return
	}
	raw, e := base64.StdEncoding.DecodeString(parts[1])
	if e != nil || len(raw) > 2<<20 {
		failure(w, 400, "invalid_image")
		return
	}
	cfg, _, e := image.DecodeConfig(bytes.NewReader(raw))
	if e != nil || cfg.Width < 1 || cfg.Height < 1 || cfg.Width > 2048 || cfg.Height > 2048 || cfg.Width*cfg.Height > 4<<20 {
		failure(w, 400, "invalid_image")
		return
	}
	img, _, e := image.Decode(bytes.NewReader(raw))
	if e != nil {
		failure(w, 400, "invalid_image")
		return
	}
	dst := image.NewNRGBA(image.Rect(0, 0, 128, 128))
	b := img.Bounds()
	for y := 0; y < 128; y++ {
		for x := 0; x < 128; x++ {
			dst.Set(x, y, img.At(b.Min.X+x*b.Dx()/128, b.Min.Y+y*b.Dy()/128))
		}
	}
	var out bytes.Buffer
	if png.Encode(&out, dst) != nil {
		failure(w, 400, "invalid_image")
		return
	}
	hash := sha256.Sum256(out.Bytes())
	name := hex.EncodeToString(hash[:]) + ".png"
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	_, e = tx.Exec(r.Context(), "INSERT INTO admin_icons(name,data) VALUES($1,$2) ON CONFLICT DO NOTHING", name, out.Bytes())
	if e == nil {
		e = s.adminAuditTx(r.Context(), tx, a, "icon.upload", name, map[string]any{})
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	reply(w, 201, map[string]string{"icon": "/v1/catalog/icons/" + name})
}
func (s *Server) catalogIcon(w http.ResponseWriter, r *http.Request) {
	name := r.PathValue("name")
	if builtinIcons[name] {
		raw, e := builtinIconFiles.ReadFile("adminassets/icons/" + name + ".svg")
		if e != nil {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "image/svg+xml")
		w.Header().Set("Cache-Control", "public,max-age=86400")
		_, _ = w.Write(raw)
		return
	}
	if !uploadedIcon.MatchString("/v1/catalog/icons/" + name) {
		http.NotFound(w, r)
		return
	}
	var raw []byte
	if s.db.QueryRow(r.Context(), "SELECT data FROM admin_icons WHERE name=$1", name).Scan(&raw) != nil {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("Cache-Control", "public,max-age=31536000,immutable")
	_, _ = w.Write(raw)
}
