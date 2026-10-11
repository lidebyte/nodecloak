package api

import (
	"crypto/sha256"
	"encoding/base64"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestOAuthPageStatesAndLanguages(t *testing.T) {
	for _, tc := range []struct {
		name, language, code, title, htmlLanguage string
		status                                    int
	}{
		{"success", "en-US,en;q=0.9", "", "Sign-in verified", "en", 200},
		{"Chinese", "en;q=0.2,zh-CN;q=0.9", "", "登录验证完成", "zh-CN", 200},
		{"cancelled", "en", "google_cancelled", "Sign-in cancelled", "en", 400},
		{"expired", "zh-CN", "invalid_oauth_state", "登录链接已失效", "zh-CN", 400},
		{"error", "en", "google_verification_failed", "Sign-in incomplete", "en", 401},
	} {
		t.Run(tc.name, func(t *testing.T) {
			r := httptest.NewRequest(http.MethodGet, "/v1/auth/google/callback?state=private-state&code=private-code", nil)
			r.Header.Set("Accept-Language", tc.language)
			w := httptest.NewRecorder()
			renderOAuthPage(w, r, tc.status, tc.code)
			body := w.Body.String()
			if w.Code != tc.status || !strings.Contains(body, `<html lang="`+tc.htmlLanguage+`">`) || !strings.Contains(body, `<h1 id="callback-title">`+tc.title+`</h1>`) {
				t.Fatalf("unexpected localized page (status %d)", w.Code)
			}
			if strings.Contains(body, "private-state") || strings.Contains(body, "private-code") || strings.Contains(body, "<script") {
				t.Fatal("page exposes OAuth query data or executes script")
			}
			if tc.status != 200 && strings.Contains(body, `status-icon success`) {
				t.Fatal("failed flow rendered as success")
			}
			if w.Header().Get("Content-Type") != "text/html; charset=utf-8" || w.Header().Get("Cache-Control") != "no-store" || w.Header().Get("Referrer-Policy") != "no-referrer" {
				t.Fatal("missing HTML privacy headers")
			}
			_, style, ok := strings.Cut(body, "<style>")
			if !ok {
				t.Fatal("missing style")
			}
			style, _, ok = strings.Cut(style, "</style>")
			if !ok {
				t.Fatal("unterminated style")
			}
			hash := sha256.Sum256([]byte(style))
			policy := w.Header().Get("Content-Security-Policy")
			for _, directive := range []string{"default-src 'none'", "style-src 'sha256-" + base64.StdEncoding.EncodeToString(hash[:]) + "'", "font-src data:", "frame-ancestors 'none'"} {
				if !strings.Contains(policy, directive) {
					t.Fatalf("CSP missing %s", directive)
				}
			}
			if !strings.Contains(style, "data:font/ttf;base64,"+base64.StdEncoding.EncodeToString(oauthBrandFont)) {
				t.Fatal("brand font must be embedded without external requests")
			}
		})
	}
}
