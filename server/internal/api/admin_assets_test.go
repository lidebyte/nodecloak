package api

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestAdminEmbeddedPageDoesNotRedirect(t *testing.T) {
	s := &Server{}
	for _, path := range []string{"/admin/", "/admin/index.html", "/admin/catalog"} {
		w := httptest.NewRecorder()
		s.adminAssets(w, httptest.NewRequest("GET", path, nil))
		if w.Code != 200 || w.Header().Get("Location") != "" || !strings.Contains(w.Body.String(), "/admin/assets/") {
			t.Fatalf("%s: status=%d location=%s", path, w.Code, w.Header().Get("Location"))
		}
	}
	w := httptest.NewRecorder()
	s.adminAssets(w, httptest.NewRequest("GET", "/admin/missing.js", nil))
	if w.Code != 404 {
		t.Fatal(w.Code)
	}
}

func TestAdminModuleOriginPolicy(t *testing.T) {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /admin/", func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(200) })
	s := &Server{mux: mux, cfg: Config{PublicURL: "https://nodecloak.com"}}
	for _, tc := range []struct {
		path, origin string
		status       int
	}{
		{"/admin/assets/app.js", "https://nodecloak.com", 200},
		{"/admin/assets/app.js", "https://evil.example", 403},
		{"/v1/me", "https://nodecloak.com", 403},
	} {
		r := httptest.NewRequest("GET", tc.path, nil)
		r.Header.Set("Origin", tc.origin)
		w := httptest.NewRecorder()
		s.Handler().ServeHTTP(w, r)
		if w.Code != tc.status {
			t.Fatalf("%s origin %s: %d", tc.path, tc.origin, w.Code)
		}
	}
}

func TestCatalogValidationMatchesDesktop(t *testing.T) {
	s := &Server{}
	r := httptest.NewRequest("PUT", "/v1/admin/catalog", nil)
	group := catalogGroup{ID: "tools", Name: "工具", NameEn: "Tools", Icon: "globe", Enabled: true, Sites: []catalogSite{{ID: "site", Name: "Site", URL: "https://nodecloak.com", Icon: "globe", Enabled: true}}}
	if !s.validateCatalog(r, []catalogGroup{group}) {
		t.Fatal("valid catalog rejected")
	}
	invalid := group
	invalid.NameEn = " "
	if s.validateCatalog(r, []catalogGroup{invalid}) {
		t.Fatal("blank English name accepted")
	}
	invalid = group
	invalid.Order = 100001
	if s.validateCatalog(r, []catalogGroup{invalid}) {
		t.Fatal("unsupported order accepted")
	}
	for _, suffix := range []string{"home", "test", "invalid", "onion"} {
		if safeDestination("https://host." + suffix) {
			t.Fatalf("private suffix %s accepted", suffix)
		}
	}
}
