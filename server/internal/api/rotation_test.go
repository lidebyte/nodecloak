package api

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"testing"
)

func TestRevocationRotationIsAtomicAndCannotLoseConcurrentObjects(t *testing.T) {
	h := setup(t)
	a := h.login("rotate@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	b := h.login("rotate@example.com", deviceB)["accessToken"].(string)
	_, ds := h.req("GET", "/v1/devices", a, nil)
	var fp string
	for _, raw := range ds["devices"].([]any) {
		d := raw.(map[string]any)
		if d["id"] == deviceB {
			fp = d["fingerprint"].(string)
		}
	}
	h.req("POST", "/v1/devices/"+deviceB+"/approve", a, map[string]any{"epoch": 1, "fingerprint": fp, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "device-b-envelope"})
	h.req("PUT", "/v1/objects/"+objectA, a, map[string]any{"kind": "config", "baseRevision": 0, "epoch": 1, "payload": "old-ciphertext"})
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{9}, 32))
	body := map[string]any{"baseEpoch": 1, "revokeDevice": deviceB, "recoveryEnvelope": "new-recovery-envelope", "recoveryVerifier": base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey)), "devices": []any{map[string]any{"id": deviceA, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "new-device-a-envelope"}}, "objects": []any{map[string]any{"id": objectA, "kind": "config", "baseRevision": 0, "payload": "new-ciphertext", "deleted": false, "generation": 0}}}
	status, _ := h.req("POST", "/v1/vault/rotate", a, body)
	if status != 409 {
		t.Fatalf("stale manifest accepted: %d", status)
	}
	if status, _ = h.req("GET", "/v1/objects", b, nil); status != 200 {
		t.Fatal("failed rotation partially revoked a device")
	}
	body["objects"].([]any)[0].(map[string]any)["baseRevision"] = 1
	status, _ = h.req("POST", "/v1/vault/rotate", a, body)
	if status != 200 {
		t.Fatalf("rotation failed %d", status)
	}
	if status, _ = h.req("GET", "/v1/objects", b, nil); status != 401 {
		t.Fatalf("revoked device retained access %d", status)
	}
	_, o := h.req("GET", "/v1/objects/"+objectA, a, nil)
	if o["epoch"] != float64(2) || o["revision"] != float64(2) || o["payload"] != "new-ciphertext" {
		t.Fatal("rotation lost object or epoch")
	}
	status, _ = h.req("PUT", "/v1/objects/"+objectA, a, map[string]any{"kind": "config", "baseRevision": 2, "epoch": 1, "payload": "stale-key"})
	if status != 409 {
		t.Fatal("old encryption epoch accepted")
	}
}
