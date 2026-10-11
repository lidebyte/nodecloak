package api

import (
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha1"
	"crypto/subtle"
	"encoding/base32"
	"encoding/base64"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"unicode/utf8"

	"github.com/jackc/pgx/v5"
	"golang.org/x/crypto/argon2"
)

var passwordWorkers = make(chan struct{}, 2)

func passwordValid(v string) bool {
	return utf8.ValidString(v) && utf8.RuneCountInString(v) >= 12 && utf8.RuneCountInString(v) <= 128 && len(v) <= 512
}
func passwordKey(password string, salt []byte) []byte {
	return argon2.IDKey([]byte(password), salt, 3, 64*1024, 2, 32)
}

// Reject excess work rather than accumulating goroutines and database locks.
func takePasswordBudget(w http.ResponseWriter) (func(), bool) {
	select {
	case passwordWorkers <- struct{}{}:
		return func() { <-passwordWorkers }, true
	default:
		failure(w, 429, "try_later")
		return nil, false
	}
}
func hashPassword(password string) string {
	salt := make([]byte, 16)
	if _, e := rand.Read(salt); e != nil {
		panic(e)
	}
	return "a2id-v1$" + base64.RawStdEncoding.EncodeToString(salt) + "$" + base64.RawStdEncoding.EncodeToString(passwordKey(password, salt))
}
func checkPassword(hash, password string) bool {
	parts := strings.Split(hash, "$")
	if len(parts) != 3 || parts[0] != "a2id-v1" || len(password) > 512 {
		return false
	}
	salt, e := base64.RawStdEncoding.DecodeString(parts[1])
	key, ke := base64.RawStdEncoding.DecodeString(parts[2])
	if e != nil || ke != nil || len(salt) != 16 || len(key) != 32 {
		return false
	}
	return subtle.ConstantTimeCompare(key, passwordKey(password, salt)) == 1
}
func decodeTOTPSecret(secret string) []byte {
	b, _ := base32.StdEncoding.WithPadding(base32.NoPadding).DecodeString(secret)
	return b
}
func totpCode(secret []byte, counter int64, digits int) string {
	var b [8]byte
	binary.BigEndian.PutUint64(b[:], uint64(counter))
	mac := hmac.New(sha1.New, secret)
	mac.Write(b[:])
	sum := mac.Sum(nil)
	offset := sum[len(sum)-1] & 15
	n := binary.BigEndian.Uint32(sum[offset:offset+4]) & 0x7fffffff
	mod := uint32(1000000)
	if digits == 8 {
		mod = 100000000
	}
	return fmt.Sprintf("%0*d", digits, n%mod)
}
func (s *Server) totpCipher() (cipher.AEAD, error) {
	key, _ := hex.DecodeString(s.hash("nodecloak-totp-encryption-v1"))
	block, e := aes.NewCipher(key)
	if e != nil {
		return nil, e
	}
	return cipher.NewGCM(block)
}
func (s *Server) sealTOTP(account, secret string) (string, error) {
	a, e := s.totpCipher()
	if e != nil {
		return "", e
	}
	nonce := make([]byte, a.NonceSize())
	if _, e = rand.Read(nonce); e != nil {
		return "", e
	}
	return base64.RawStdEncoding.EncodeToString(a.Seal(nonce, nonce, []byte(secret), []byte("nodecloak-totp-v1:"+account))), nil
}
func (s *Server) openTOTP(account, encrypted string) (string, error) {
	a, e := s.totpCipher()
	if e != nil {
		return "", e
	}
	b, e := base64.RawStdEncoding.DecodeString(encrypted)
	if e != nil || len(b) < a.NonceSize()+a.Overhead() {
		return "", fmt.Errorf("invalid TOTP ciphertext")
	}
	plain, e := a.Open(nil, b[:a.NonceSize()], b[a.NonceSize():], []byte("nodecloak-totp-v1:"+account))
	return string(plain), e
}
func (s *Server) backupHash(account, code string) string {
	return s.hash("2fa-backup:" + account + ":" + strings.ToUpper(strings.NewReplacer("-", "", " ", "").Replace(strings.TrimSpace(code))))
}
func (s *Server) backupCodes(account string) ([]string, []string) {
	codes, hashes := make([]string, 10), make([]string, 10)
	for i := range codes {
		b := make([]byte, 16)
		if _, e := rand.Read(b); e != nil {
			panic(e)
		}
		v := strings.ToUpper(hex.EncodeToString(b))
		codes[i] = v[:8] + "-" + v[8:16] + "-" + v[16:24] + "-" + v[24:]
		hashes[i] = s.backupHash(account, codes[i])
	}
	return codes, hashes
}
func (s *Server) acceptFactor(ctx context.Context, tx pgx.Tx, account, code string, backups bool) bool {
	if len(code) > 64 {
		return false
	}
	code = strings.TrimSpace(code)
	var encrypted string
	var last int64
	var hashes []string
	if tx.QueryRow(ctx, "SELECT totp_secret,totp_last_counter,backup_hashes FROM users WHERE id=$1 FOR UPDATE", account).Scan(&encrypted, &last, &hashes) != nil || encrypted == "" {
		return false
	}
	secret, e := s.openTOTP(account, encrypted)
	if e != nil {
		return false
	}
	now := s.now().Unix() / 30
	if len(code) == 6 {
		for counter := now - 1; counter <= now+1; counter++ {
			if counter > last && subtle.ConstantTimeCompare([]byte(totpCode(decodeTOTPSecret(secret), counter, 6)), []byte(code)) == 1 {
				_, e = tx.Exec(ctx, "UPDATE users SET totp_last_counter=$2 WHERE id=$1", account, counter)
				return e == nil
			}
		}
	}
	if backups {
		hash := s.backupHash(account, code)
		for _, v := range hashes {
			if subtle.ConstantTimeCompare([]byte(hash), []byte(v)) == 1 {
				_, e = tx.Exec(ctx, "UPDATE users SET backup_hashes=array_remove(backup_hashes,$2) WHERE id=$1", account, v)
				return e == nil
			}
		}
	}
	return false
}

func (s *Server) passwordLogin(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Email    string      `json:"email"`
		Password string      `json:"password"`
		Device   deviceInput `json:"device"`
	}
	if !decode(w, r, &in, 8192) {
		return
	}
	email, valid := normalizedEmail(in.Email)
	if !valid || !in.Device.valid() || len(in.Password) > 512 {
		failure(w, 401, "invalid_credentials")
		return
	}
	if !s.limit(r.Context(), "password-email:"+email, 5) || !s.limit(r.Context(), "password-ip:"+s.clientIP(r), 20) {
		failure(w, 429, "try_later")
		return
	}
	release, ok := takePasswordBudget(w)
	if !ok {
		return
	}
	defer release()
	var hash string
	_ = s.db.QueryRow(r.Context(), "SELECT password_hash FROM users WHERE email=$1", email).Scan(&hash)
	if hash == "" {
		passwordKey(in.Password, make([]byte, 16))
		failure(w, 401, "invalid_credentials")
		return
	}
	if !checkPassword(hash, in.Password) {
		failure(w, 401, "invalid_credentials")
		return
	}
	s.completeVerifiedLogin(w, r, email, "", in.Device, s.hash("device:"+in.Device.Secret), "password", hash, -1)
}

func (s *Server) createLoginChallenge(ctx context.Context, tx pgx.Tx, account, method string, version int, d deviceInput, secretHash string) (string, error) {
	d.Secret = secretHash
	device, e := json.Marshal(d)
	if e != nil {
		return "", e
	}
	challenge := token()
	// Bound outstanding challenges per account, including attempts from different IPs.
	_, e = tx.Exec(ctx, "DELETE FROM login_challenges WHERE account_id=$1 AND expires_at<now()", account)
	if e != nil {
		return "", e
	}
	var count int
	if tx.QueryRow(ctx, "SELECT count(*) FROM login_challenges WHERE account_id=$1", account).Scan(&count) != nil || count >= 10 {
		return "", fmt.Errorf("challenge limit")
	}
	_, e = tx.Exec(ctx, "INSERT INTO login_challenges(token_hash,account_id,device,method,security_version,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '5 minutes')", s.hash(challenge), account, device, method, version)
	return challenge, e
}
func (s *Server) twoFactorLogin(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Challenge string `json:"challenge"`
		Code      string `json:"code"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	if len(in.Challenge) != 43 || len(in.Code) > 64 || !s.limit(r.Context(), "factor-ip:"+s.clientIP(r), 20) {
		failure(w, 401, "invalid_code")
		return
	}
	var accountID string
	if s.db.QueryRow(r.Context(), "SELECT account_id::text FROM login_challenges WHERE token_hash=$1", s.hash(in.Challenge)).Scan(&accountID) != nil {
		failure(w, 401, "invalid_code")
		return
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	defer tx.Rollback(r.Context())
	var email string
	var version int
	if tx.QueryRow(r.Context(), "SELECT email,security_version FROM users WHERE id=$1 FOR UPDATE", accountID).Scan(&email, &version) != nil {
		failure(w, 401, "invalid_code")
		return
	}
	var device []byte
	var method string
	var expected, attempts int
	var live bool
	if tx.QueryRow(r.Context(), "SELECT device,method,security_version,attempts,expires_at>now() FROM login_challenges WHERE token_hash=$1 AND account_id=$2 FOR UPDATE", s.hash(in.Challenge), accountID).Scan(&device, &method, &expected, &attempts, &live) != nil || expected != version || attempts >= 5 || !live {
		failure(w, 401, "invalid_code")
		return
	}
	if !s.acceptFactor(r.Context(), tx, accountID, in.Code, true) {
		_, _ = tx.Exec(r.Context(), "UPDATE login_challenges SET attempts=attempts+1 WHERE token_hash=$1", s.hash(in.Challenge))
		_ = tx.Commit(r.Context())
		failure(w, 401, "invalid_code")
		return
	}
	var d deviceInput
	if json.Unmarshal(device, &d) != nil {
		failure(w, 401, "invalid_code")
		return
	}
	_, e = tx.Exec(r.Context(), "DELETE FROM login_challenges WHERE token_hash=$1", s.hash(in.Challenge))
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.completeVerifiedLogin(w, r, email, "", d, d.Secret, method, "", version)
}

type securityProof struct {
	CurrentPassword string `json:"currentPassword"`
	Code            string `json:"code"`
}

func (s *Server) securityTx(w http.ResponseWriter, r *http.Request, i identity) (pgx.Tx, func(), bool) {
	if !s.limit(r.Context(), "security-account:"+i.Account, 10) || !s.limit(r.Context(), "security-ip:"+s.clientIP(r), 30) {
		failure(w, 429, "try_later")
		return nil, nil, false
	}
	release, ok := takePasswordBudget(w)
	if !ok {
		return nil, nil, false
	}
	tx, e := s.db.Begin(r.Context())
	if e != nil {
		release()
		failure(w, 503, "database_unavailable")
		return nil, nil, false
	}
	if s.authorizeTx(r.Context(), tx, i, false) != nil {
		tx.Rollback(r.Context())
		release()
		failure(w, 401, "unauthorized")
		return nil, nil, false
	}
	return tx, release, true
}
func (s *Server) verifySecurityProof(w http.ResponseWriter, r *http.Request, tx pgx.Tx, i identity, p securityProof) bool {
	var password, totp string
	var recent bool
	if tx.QueryRow(r.Context(), "SELECT password_hash,totp_secret FROM users WHERE id=$1", i.Account).Scan(&password, &totp) != nil {
		failure(w, 503, "database_unavailable")
		return false
	}
	if password != "" {
		if !checkPassword(password, p.CurrentPassword) {
			failure(w, 403, "authentication_failed")
			return false
		}
	} else {
		if tx.QueryRow(r.Context(), "SELECT auth_at>now()-interval '15 minutes' AND auth_method IN ('google','email','password') FROM sessions WHERE id=$1", i.Session).Scan(&recent) != nil || !recent {
			failure(w, 403, "reauthentication_required")
			return false
		}
	}
	if totp != "" && !s.acceptFactor(r.Context(), tx, i.Account, p.Code, true) {
		failure(w, 403, "invalid_code")
		return false
	}
	return true
}
func (s *Server) finishSecurityWrite(ctx context.Context, tx pgx.Tx, i identity) error {
	_, e := tx.Exec(ctx, "UPDATE users SET security_version=security_version+1 WHERE id=$1;", i.Account)
	if e != nil {
		return e
	}
	_, e = tx.Exec(ctx, "UPDATE sessions SET revoked=true WHERE account_id=$1 AND id<>$2", i.Account, i.Session)
	if e != nil {
		return e
	}
	_, e = tx.Exec(ctx, "UPDATE sessions SET auth_at=now(),auth_method='password' WHERE id=$1", i.Session)
	if e != nil {
		return e
	}
	_, e = tx.Exec(ctx, "DELETE FROM login_challenges WHERE account_id=$1", i.Account)
	if e != nil {
		return e
	}
	_, e = tx.Exec(ctx, "DELETE FROM totp_setups WHERE account_id=$1", i.Account)
	if e != nil {
		return e
	}
	return tx.Commit(ctx)
}
func (s *Server) setPassword(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		securityProof
		Password string `json:"password"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	if !passwordValid(in.Password) {
		failure(w, 400, "password_length")
		return
	}
	tx, release, ok := s.securityTx(w, r, i)
	if !ok {
		return
	}
	defer release()
	defer tx.Rollback(r.Context())
	if !s.verifySecurityProof(w, r, tx, i, in.securityProof) {
		return
	}
	_, e := tx.Exec(r.Context(), "UPDATE users SET password_hash=$2 WHERE id=$1", i.Account, hashPassword(in.Password))
	if e == nil {
		e = s.finishSecurityWrite(r.Context(), tx, i)
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.audit(r.Context(), i, "password_changed", "")
	s.account(w, r, i)
}
func (s *Server) unlinkGoogle(w http.ResponseWriter, r *http.Request, i identity) {
	var p securityProof
	if !decode(w, r, &p, 4096) {
		return
	}
	tx, release, ok := s.securityTx(w, r, i)
	if !ok {
		return
	}
	defer release()
	defer tx.Rollback(r.Context())
	var hasPassword bool
	if tx.QueryRow(r.Context(), "SELECT password_hash<>'' FROM users WHERE id=$1", i.Account).Scan(&hasPassword) != nil || !hasPassword {
		failure(w, 409, "password_required")
		return
	}
	if !s.verifySecurityProof(w, r, tx, i, p) {
		return
	}
	_, e := tx.Exec(r.Context(), "UPDATE users SET google_sub=NULL WHERE id=$1", i.Account)
	if e == nil {
		e = s.finishSecurityWrite(r.Context(), tx, i)
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.audit(r.Context(), i, "google_unlinked", "")
	s.account(w, r, i)
}
func (s *Server) startTwoFactor(w http.ResponseWriter, r *http.Request, i identity) {
	var p securityProof
	if !decode(w, r, &p, 4096) {
		return
	}
	tx, release, ok := s.securityTx(w, r, i)
	if !ok {
		return
	}
	defer release()
	defer tx.Rollback(r.Context())
	var password, totp, email string
	var version int
	if tx.QueryRow(r.Context(), "SELECT password_hash,totp_secret,email,security_version FROM users WHERE id=$1", i.Account).Scan(&password, &totp, &email, &version) != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	if password == "" {
		failure(w, 409, "password_required")
		return
	}
	if totp != "" {
		failure(w, 409, "two_factor_enabled")
		return
	}
	if !s.verifySecurityProof(w, r, tx, i, p) {
		return
	}
	b := make([]byte, 20)
	if _, e := rand.Read(b); e != nil {
		failure(w, 503, "service_error")
		return
	}
	secret := base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(b)
	encrypted, e := s.sealTOTP(i.Account, secret)
	if e == nil {
		_, e = tx.Exec(r.Context(), "INSERT INTO totp_setups(account_id,session_id,secret,security_version,expires_at) VALUES($1,$2,$3,$4,now()+interval '10 minutes') ON CONFLICT(account_id) DO UPDATE SET session_id=$2,secret=$3,security_version=$4,expires_at=now()+interval '10 minutes',attempts=0", i.Account, i.Session, encrypted, version)
	}
	if e == nil {
		e = tx.Commit(r.Context())
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	q := url.Values{"secret": {secret}, "issuer": {"NodeCloak"}, "algorithm": {"SHA1"}, "digits": {"6"}, "period": {"30"}}
	reply(w, 200, map[string]string{"secret": secret, "uri": "otpauth://totp/" + url.PathEscape("NodeCloak:"+email) + "?" + q.Encode()})
}
func (s *Server) enableTwoFactor(w http.ResponseWriter, r *http.Request, i identity) {
	var in struct {
		Code string `json:"code"`
	}
	if !decode(w, r, &in, 4096) {
		return
	}
	tx, release, ok := s.securityTx(w, r, i)
	if !ok {
		return
	}
	defer release()
	defer tx.Rollback(r.Context())
	var encrypted string
	var version, expected, attempts int
	var live bool
	if tx.QueryRow(r.Context(), "SELECT security_version FROM users WHERE id=$1", i.Account).Scan(&version) != nil || tx.QueryRow(r.Context(), "SELECT secret,security_version,attempts,expires_at>now() FROM totp_setups WHERE account_id=$1 AND session_id=$2 FOR UPDATE", i.Account, i.Session).Scan(&encrypted, &expected, &attempts, &live) != nil || version != expected || attempts >= 5 || !live {
		failure(w, 401, "invalid_code")
		return
	}
	secret, e := s.openTOTP(i.Account, encrypted)
	matched := int64(-1)
	now := s.now().Unix() / 30
	if e == nil && len(in.Code) == 6 {
		for counter := now - 1; counter <= now+1; counter++ {
			if subtle.ConstantTimeCompare([]byte(in.Code), []byte(totpCode(decodeTOTPSecret(secret), counter, 6))) == 1 {
				matched = counter
				break
			}
		}
	}
	if matched < 0 {
		_, _ = tx.Exec(r.Context(), "UPDATE totp_setups SET attempts=attempts+1 WHERE account_id=$1", i.Account)
		_ = tx.Commit(r.Context())
		failure(w, 401, "invalid_code")
		return
	}
	codes, hashes := s.backupCodes(i.Account)
	_, e = tx.Exec(r.Context(), "UPDATE users SET totp_secret=$2,totp_last_counter=$3,backup_hashes=$4 WHERE id=$1 AND totp_secret=''", i.Account, encrypted, matched, hashes)
	if e == nil {
		e = s.finishSecurityWrite(r.Context(), tx, i)
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.audit(r.Context(), i, "two_factor_enabled", "")
	reply(w, 200, map[string]any{"backupCodes": codes})
}
func (s *Server) disableTwoFactor(w http.ResponseWriter, r *http.Request, i identity) {
	var p securityProof
	if !decode(w, r, &p, 4096) {
		return
	}
	tx, release, ok := s.securityTx(w, r, i)
	if !ok {
		return
	}
	defer release()
	defer tx.Rollback(r.Context())
	if !s.verifySecurityProof(w, r, tx, i, p) {
		return
	}
	_, e := tx.Exec(r.Context(), "UPDATE users SET totp_secret='',totp_last_counter=-1,backup_hashes='{}' WHERE id=$1", i.Account)
	if e == nil {
		e = s.finishSecurityWrite(r.Context(), tx, i)
	}
	if e != nil {
		failure(w, 503, "database_unavailable")
		return
	}
	s.audit(r.Context(), i, "two_factor_disabled", "")
	s.account(w, r, i)
}
