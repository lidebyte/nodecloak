package api

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"strings"
	"sync"
	"testing"
	"time"
)

const shareA = "77777777-7777-4777-8777-777777777777"
const deviceC = "44444444-4444-4444-8444-444444444444"

func sharingUsers(t *testing.T) (*harness, string, string, string) {
	h := setup(t)
	owner := h.login("sender@example.com", deviceA)["accessToken"].(string)
	h.enroll(owner)
	friendLogin := h.login("friend@example.com", deviceB)
	friend := friendLogin["accessToken"].(string)
	h.enroll(friend)
	id := friendLogin["account"].(map[string]any)["id"].(string)
	return h, owner, friend, id
}
func sharingBody(recipient string, deadline time.Time) map[string]any {
	return map[string]any{"id": shareA, "sourceProfileId": "local-profile-123", "profileName": "Example", "browser": "chrome", "recipientId": recipient, "recipientName": "friend", "recipientEmail": "friend@example.com", "expiresAt": deadline, "includesSession": false, "note": "hello", "payload": "opaque-ciphertext", "writer": deviceA, "envelopes": []any{map[string]any{"deviceId": deviceB, "publicKey": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceB[0])}, 32)), "envelope": "wrapped-share-key"}}}
}
func TestSharingLifecycleAndIsolation(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	if s, _ := h.req("GET", "/v1/shares", "", nil); s != 401 {
		t.Fatalf("unauthenticated: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/recipient", owner, map[string]any{"name": "wrong", "email": "friend@example.com"}); s != 404 {
		t.Fatalf("name mismatch: %d", s)
	}
	if s, b := h.req("POST", "/v1/shares/recipient", owner, map[string]any{"name": "friend", "email": "FRIEND@example.com"}); s != 200 || len(b["devices"].([]any)) != 1 {
		t.Fatalf("lookup: %d %v", s, b)
	}
	if s, b := h.req("POST", "/v1/shares", owner, sharingBody(id, time.Now().Add(time.Hour))); s != 201 || b["share"].(map[string]any)["status"] != "pending" {
		t.Fatalf("create: %d %v", s, b)
	}
	if s, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); s != 409 {
		t.Fatalf("preaccept payload: %d", s)
	}
	_, b := h.req("GET", "/v1/shares", friend, nil)
	encoded, _ := json.Marshal(b)
	if bytes.Contains(encoded, []byte("opaque-ciphertext")) || bytes.Contains(encoded, []byte("wrapped-share-key")) {
		t.Fatal("list leaked package")
	}
	other := h.login("other@example.com", deviceC)["accessToken"].(string)
	h.enroll(other)
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", other, nil); s != 404 {
		t.Fatalf("other accept: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", owner, nil); s != 404 {
		t.Fatalf("owner accept: %d", s)
	}
	for n := 0; n < 2; n++ {
		if s, b := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); s != 200 || b["share"].(map[string]any)["status"] != "accepted" {
			t.Fatalf("accept retry: %d %v", s, b)
		}
	}
	if s, b := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); s != 200 || b["payload"] != "opaque-ciphertext" || b["envelope"] != "wrapped-share-key" {
		t.Fatalf("package: %d %v", s, b)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 200 {
		t.Fatalf("authorize: %d", s)
	}
	if s, _ := h.req("PATCH", "/v1/shares/"+shareA, friend, map[string]any{"expiresAt": time.Now().Add(2 * time.Hour)}); s != 404 {
		t.Fatalf("recipient edit: %d", s)
	}
	if s, _ := h.req("PATCH", "/v1/shares/"+shareA, owner, map[string]any{"expiresAt": time.Now().Add(2 * time.Hour)}); s != 200 {
		t.Fatalf("owner edit: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/revoke", owner, nil); s != 200 {
		t.Fatalf("revoke: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 409 {
		t.Fatalf("revoked authorize: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); s != 409 {
		t.Fatalf("revoked accept: %d", s)
	}
}
func TestSharingExpiryAndTerminalStates(t *testing.T) {
	for _, terminal := range []string{"decline", "leave", "expire"} {
		t.Run(terminal, func(t *testing.T) {
			h, owner, friend, id := sharingUsers(t)
			now := time.Now().UTC()
			h.s.now = func() time.Time { return now }
			if s, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, now.Add(time.Hour))); s != 201 {
				t.Fatalf("create: %d", s)
			}
			if terminal != "decline" {
				h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil)
			}
			if terminal == "expire" {
				now = now.Add(time.Hour)
			} else if s, _ := h.req("POST", "/v1/shares/"+shareA+"/"+terminal, friend, nil); s != 200 {
				t.Fatalf("terminal: %d", s)
			}
			if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 409 {
				t.Fatalf("terminal authorize: %d", s)
			}
			if s, _ := h.req("PATCH", "/v1/shares/"+shareA, owner, map[string]any{"expiresAt": now.Add(time.Hour)}); s != 409 {
				t.Fatalf("terminal resurrected: %d", s)
			}
			if terminal == "expire" {
				_, b := h.req("GET", "/v1/shares", friend, nil)
				if b["received"].([]any)[0].(map[string]any)["status"] != "expired" {
					t.Fatalf("expiry missing: %v", b)
				}
			}
		})
	}
}
func TestSharingDeviceBindingAndValidation(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	body := sharingBody(id, time.Now().Add(time.Hour))
	body["recipientName"] = "wrong"
	if s, _ := h.req("POST", "/v1/shares", owner, body); s != 404 {
		t.Fatalf("recipient spoof: %d", s)
	}
	body["recipientName"] = "friend"
	envelopes := body["envelopes"].([]any)
	envelopes[0].(map[string]any)["publicKey"] = "wrong"
	if s, _ := h.req("POST", "/v1/shares", owner, body); s != 409 {
		t.Fatalf("key mismatch: %d", s)
	}
	envelopes[0].(map[string]any)["publicKey"] = base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{byte(deviceB[0])}, 32))
	body["expiresAt"] = time.Now().Add(-time.Second)
	if s, _ := h.req("POST", "/v1/shares", owner, body); s != 400 {
		t.Fatalf("past deadline: %d", s)
	}
	body["expiresAt"] = time.Now().Add(time.Hour)
	if s, _ := h.req("POST", "/v1/shares", owner, body); s != 201 {
		t.Fatalf("create: %d", s)
	}
	h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil)
	newDevice := h.login("friend@example.com", deviceC)["accessToken"].(string)
	_, _ = h.db.Exec(context.Background(), "UPDATE devices SET status='approved' WHERE account_id=$1 AND id=$2", id, deviceC)
	if s, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", newDevice, nil); s != 403 {
		t.Fatalf("unaddressed device: %d", s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE devices SET public_key='changed-key' WHERE account_id=$1 AND id=$2", id, deviceB)
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 403 {
		t.Fatalf("changed device key: %d", s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE users SET disabled=true WHERE id=$1", id)
	if s, _ := h.req("GET", "/v1/shares", friend, nil); s != 401 {
		t.Fatalf("disabled account: %d", s)
	}
}
func TestSharingAcceptRevokeRace(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	if s, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, time.Now().Add(time.Hour))); s != 201 {
		t.Fatal(s)
	}
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil)
		if s != 200 && s != 409 {
			t.Errorf("accept race: %d", s)
		}
	}()
	go func() {
		defer wg.Done()
		s, _ := h.req("POST", "/v1/shares/"+shareA+"/revoke", owner, nil)
		if s != 200 {
			t.Errorf("revoke race: %d", s)
		}
	}()
	wg.Wait()
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 409 {
		t.Fatalf("race resurrected: %d", s)
	}
}

func TestSharingPendingExpiryAndSenderDisabled(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	now := time.Now().UTC()
	h.s.now = func() time.Time { return now }
	if s, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, now.Add(time.Hour))); s != 201 {
		t.Fatal(s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); s != 409 {
		t.Fatalf("pending authorization: %d", s)
	}
	if s, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", owner, nil); s != 404 {
		t.Fatalf("sender package access: %d", s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE users SET disabled=true WHERE email='sender@example.com'")
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); s != 403 {
		t.Fatalf("disabled sender accepted: %d", s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE users SET disabled=false WHERE email='sender@example.com'")
	now = now.Add(time.Hour)
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); s != 409 {
		t.Fatalf("expired pending accepted: %d", s)
	}
	var payload, status string
	if e := h.db.QueryRow(context.Background(), "SELECT payload,status FROM profile_shares WHERE id=$1", shareA).Scan(&payload, &status); e != nil || payload != "" || status != "expired" {
		t.Fatalf("expired package retained: %s %s %v", payload, status, e)
	}
}
func TestSharingPendingDeviceCannotAccept(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	if s, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, time.Now().Add(time.Hour))); s != 201 {
		t.Fatal(s)
	}
	_, _ = h.db.Exec(context.Background(), "UPDATE devices SET status='pending' WHERE account_id=$1 AND id=$2", id, deviceB)
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); s != 403 {
		t.Fatalf("pending device accept: %d", s)
	}
	if s, _ := h.req("POST", "/v1/shares/"+shareA+"/decline", friend, nil); s != 200 {
		t.Fatalf("pending device metadata action: %d", s)
	}
}

func TestSharingUnicodeNamesAndBrowserValidation(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	name := strings.Repeat("友", 60)
	if s, b := h.req("PATCH", "/v1/account", friend, map[string]any{"name": name}); s != 200 {
		t.Fatalf("valid account name: %d %v", s, b)
	}
	if s, b := h.req("POST", "/v1/shares/recipient", owner, map[string]any{"name": name, "email": "friend@example.com"}); s != 200 || b["account"].(map[string]any)["name"] != name {
		t.Fatalf("unicode recipient pair: %d %v", s, b)
	}
	body := sharingBody(id, time.Now().Add(time.Hour))
	body["recipientName"] = name
	body["profileName"] = strings.Repeat("😀", 60)
	body["browser"] = "unknown-browser"
	if s, _ := h.req("POST", "/v1/shares", owner, body); s != 400 {
		t.Fatalf("unknown browser: %d", s)
	}
	body["browser"] = "chrome"
	if s, b := h.req("POST", "/v1/shares", owner, body); s != 201 || b["share"].(map[string]any)["recipientName"] != name || b["share"].(map[string]any)["profileName"] != body["profileName"] {
		t.Fatalf("unicode share metadata: %d %v", s, b)
	}
}

func TestSharingExpiryPurgesKeysViaEveryEntryPoint(t *testing.T) {
	for _, entry := range []string{"list", "quota", "authorize"} {
		t.Run(entry, func(t *testing.T) {
			h, owner, friend, id := sharingUsers(t)
			now := time.Now().UTC()
			h.s.now = func() time.Time { return now }
			if status, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, now.Add(time.Hour))); status != 201 {
				t.Fatalf("create: %d", status)
			}
			now = now.Add(time.Hour)
			switch entry {
			case "list":
				if status, _ := h.req("GET", "/v1/shares", friend, nil); status != 200 {
					t.Fatalf("list: %d", status)
				}
			case "quota":
				body := sharingBody(id, now.Add(time.Hour))
				body["id"] = uuid()
				if status, _ := h.req("POST", "/v1/shares", owner, body); status != 201 {
					t.Fatalf("create after expiry: %d", status)
				}
			case "authorize":
				if status, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); status != 409 {
					t.Fatalf("authorize: %d", status)
				}
			}
			var envelopes int
			if err := h.db.QueryRow(context.Background(), "SELECT count(*) FROM profile_share_envelopes WHERE share_id=$1", shareA).Scan(&envelopes); err != nil || envelopes != 0 {
				t.Fatalf("expired wrapped keys retained: %d %v", envelopes, err)
			}
		})
	}
}

func TestSharingInvitationBeforeDeviceApprovalAndShareOnlyConsent(t *testing.T) {
	h := setup(t)
	ownerLogin := h.login("sender@example.com", deviceA)
	owner := ownerLogin["accessToken"].(string)
	friendLogin := h.login("friend@example.com", deviceB)
	friend := friendLogin["accessToken"].(string)
	id := friendLogin["account"].(map[string]any)["id"].(string)
	if status, response := h.req("POST", "/v1/shares/recipient", owner, map[string]any{"name": "friend", "email": "friend@example.com"}); status != 200 || len(response["devices"].([]any)) != 1 {
		t.Fatalf("pending recipient lookup: %d %v", status, response)
	}
	for _, grant := range []string{shareA, "88888888-8888-4888-8888-888888888888"} {
		body := sharingBody(id, time.Now().Add(time.Hour))
		body["id"] = grant
		if status, response := h.req("POST", "/v1/shares", owner, body); status != 201 {
			t.Fatalf("invite before device approval: %d %v", status, response)
		}
	}
	if status, response := h.req("GET", "/v1/shares", friend, nil); status != 200 || response["received"].([]any)[0].(map[string]any)["deviceAccess"] != "authorizationRequired" {
		t.Fatalf("pending invitation guidance: %d %v", status, response)
	}
	if status, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); status != 409 {
		t.Fatalf("payload before consent: %d", status)
	}
	if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); status != 403 {
		t.Fatalf("accept without explicit authorization: %d", status)
	}
	if status, response := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, map[string]any{"authorizeDevice": true}); status != 200 || response["share"].(map[string]any)["status"] != "accepted" {
		t.Fatalf("authorize and accept: %d %v", status, response)
	}
	for _, request := range []struct{ method, path string }{{"GET", "/payload"}, {"POST", "/authorize"}} {
		if status, response := h.req(request.method, "/v1/shares/"+shareA+request.path, friend, nil); status != 200 {
			t.Fatalf("authorized share access: %d %v", status, response)
		}
	}
	if status, _ := h.req("POST", "/v1/shares/88888888-8888-4888-8888-888888888888/accept", friend, nil); status != 403 {
		t.Fatalf("consent leaked to another grant: %d", status)
	}
	var status string
	var vaults int
	if err := h.db.QueryRow(context.Background(), "SELECT status FROM devices WHERE account_id=$1 AND id=$2", id, deviceB).Scan(&status); err != nil || status != "pending" {
		t.Fatalf("personal cloud approval changed: %s %v", status, err)
	}
	if err := h.db.QueryRow(context.Background(), "SELECT count(*) FROM vaults").Scan(&vaults); err != nil || vaults != 0 {
		t.Fatalf("personal vault unexpectedly initialized: %d %v", vaults, err)
	}
	newDevice := h.login("friend@example.com", deviceC)["accessToken"].(string)
	if status, response := h.req("GET", "/v1/shares", newDevice, nil); status != 200 || response["received"].([]any)[0].(map[string]any)["deviceAccess"] != "unavailable" {
		t.Fatalf("unaddressed device guidance: %d %v", status, response)
	}
	if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", newDevice, map[string]any{"authorizeDevice": true}); status != 403 {
		t.Fatalf("unaddressed device consent bypass: %d", status)
	}
	if status, _ := h.req("POST", "/v1/shares/"+shareA+"/revoke", owner, nil); status != 200 {
		t.Fatalf("pending owner revoke: %d", status)
	}
	if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, map[string]any{"authorizeDevice": true}); status != 409 {
		t.Fatalf("consent reactivated revoked grant: %d", status)
	}
}

func TestSharingExplicitConsentRetainsDeviceAndAccountRevocation(t *testing.T) {
	for _, change := range []string{"key", "device", "sender"} {
		t.Run(change, func(t *testing.T) {
			h := setup(t)
			owner := h.login("sender@example.com", deviceA)["accessToken"].(string)
			login := h.login("friend@example.com", deviceB)
			friend := login["accessToken"].(string)
			id := login["account"].(map[string]any)["id"].(string)
			if status, _ := h.req("POST", "/v1/shares", owner, sharingBody(id, time.Now().Add(time.Hour))); status != 201 {
				t.Fatal(status)
			}
			if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, map[string]any{"authorizeDevice": "true"}); status != 400 {
				t.Fatalf("non-boolean consent accepted: %d", status)
			}
			if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, map[string]any{"authorizeDevice": true}); status != 200 {
				t.Fatal(status)
			}
			expected := 403
			switch change {
			case "key":
				_, _ = h.db.Exec(context.Background(), "UPDATE devices SET public_key='changed-key' WHERE account_id=$1 AND id=$2", id, deviceB)
			case "device":
				_, _ = h.db.Exec(context.Background(), "UPDATE devices SET status='revoked' WHERE account_id=$1 AND id=$2", id, deviceB)
				expected = 401
			case "sender":
				_, _ = h.db.Exec(context.Background(), "UPDATE users SET disabled=true WHERE email='sender@example.com'")
			}
			if status, _ := h.req("POST", "/v1/shares/"+shareA+"/authorize", friend, nil); status != expected {
				t.Fatalf("authorization survived %s change: %d", change, status)
			}
			if status, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); status != expected {
				t.Fatalf("payload survived %s change: %d", change, status)
			}
		})
	}
}
