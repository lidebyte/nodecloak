package api

import (
	"bytes"
	"crypto/sha256"
	_ "embed"
	"encoding/base64"
	"html/template"
	"net/http"
	"strings"

	"golang.org/x/text/language"
)

//go:embed oauth_page.html
var oauthPageHTML string

//go:embed oauth_page.css
var oauthPageCSS string

//go:embed brand/sora.ttf
var oauthBrandFont []byte

var oauthPageTemplate = template.Must(template.New("oauth-page").Parse(oauthPageHTML))
var oauthPageStyle = strings.ReplaceAll(oauthPageCSS, "/v1/auth/brand/sora.ttf", "data:font/ttf;base64,"+base64.StdEncoding.EncodeToString(oauthBrandFont))
var oauthPageLanguages = language.NewMatcher([]language.Tag{language.English, language.SimplifiedChinese})

func renderOAuthPage(w http.ResponseWriter, r *http.Request, status int, code string) {
	// MatchStrings handles weighted browser preferences, including multiple languages.
	_, locale := language.MatchStrings(oauthPageLanguages, r.Header.Get("Accept-Language"))
	data := struct {
		Language, Title, Description, Hint string
		Style                              template.CSS
		Success                            bool
	}{Language: "en", Style: template.CSS(oauthPageStyle), Success: status == http.StatusOK}
	if locale == 1 {
		data.Language = "zh-CN"
		data.Title, data.Description, data.Hint = "登录验证完成", "请返回 NodeCloak 继续，应用会自动接收验证结果。", "可以关闭此页面，回到 NodeCloak。"
		if !data.Success {
			data.Title, data.Description, data.Hint = "登录未完成", "本次登录无法完成，请返回 NodeCloak 重新开始。", "可以关闭此页面，在应用中重新登录。"
			if code == "google_cancelled" {
				data.Title, data.Description = "已取消登录", "你已取消本次登录，可以返回 NodeCloak。"
			}
			if code == "invalid_oauth_state" {
				data.Title, data.Description = "登录链接已失效", "此链接已使用或已过期，请从 NodeCloak 重新发起登录。"
			}
		}
	} else {
		data.Title, data.Description, data.Hint = "Sign-in verified", "Return to NodeCloak to continue. The app will receive the verification result automatically.", "You can close this tab and return to NodeCloak."
		if !data.Success {
			data.Title, data.Description, data.Hint = "Sign-in incomplete", "We could not complete this sign-in. Return to NodeCloak and try again.", "You can close this tab and restart sign-in in the app."
			if code == "google_cancelled" {
				data.Title, data.Description = "Sign-in cancelled", "You cancelled this sign-in. You can return to NodeCloak."
			}
			if code == "invalid_oauth_state" {
				data.Title, data.Description = "Sign-in link expired", "This link has been used or has expired. Start a new sign-in from NodeCloak."
			}
		}
	}
	var body bytes.Buffer
	if err := oauthPageTemplate.Execute(&body, data); err != nil {
		failure(w, 500, "page_unavailable")
		return
	}
	styleHash := sha256.Sum256([]byte(oauthPageStyle))
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Header().Set("Referrer-Policy", "no-referrer")
	w.Header().Set("Content-Security-Policy", "default-src 'none'; style-src 'sha256-"+base64.StdEncoding.EncodeToString(styleHash[:])+"'; font-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
	w.WriteHeader(status)
	_, _ = w.Write(body.Bytes())
}
