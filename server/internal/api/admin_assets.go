package api

import (
	"bytes"
	"embed"
	"io/fs"
	"net/http"
	"path"
	"strings"
	"time"
)

//go:embed adminassets/web
var adminWeb embed.FS

//go:embed adminassets/default-catalog.json
var initialCatalog []byte

//go:embed adminassets/icons
var builtinIconFiles embed.FS

func (s *Server) adminAssets(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self'; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'")
	name := strings.TrimPrefix(path.Clean(strings.TrimPrefix(r.URL.Path, "/admin/")), "/")
	if name == "" || name == "." {
		name = "index.html"
	}
	files, _ := fs.Sub(adminWeb, "adminassets/web")
	if _, e := fs.Stat(files, name); e != nil {
		if strings.Contains(path.Base(name), ".") {
			http.NotFound(w, r)
			return
		}
		name = "index.html"
	}
	if name == "index.html" {
		data, err := fs.ReadFile(files, name)
		if err != nil {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		http.ServeContent(w, r, name, time.Time{}, bytes.NewReader(data))
		return
	}
	clone := r.Clone(r.Context())
	clone.URL.Path = "/" + name
	http.FileServer(http.FS(files)).ServeHTTP(w, clone)
}
