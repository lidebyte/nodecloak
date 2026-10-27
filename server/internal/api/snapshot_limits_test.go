package api

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestLargeOpaqueSessionPreservesCASAndCompleteDownload(t *testing.T) {
	h := setup(t)
	token := h.login("large-snapshot@example.com", deviceA)["accessToken"].(string)
	h.enroll(token)
	if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, map[string]any{"kind": "config", "baseRevision": 0, "epoch": 1, "payload": "config"}); status != 200 {
		t.Fatal(status)
	}
	_, lease := h.req("POST", "/v1/objects/"+objectA+"/lease", token, map[string]any{"takeover": false})
	payload := strings.Repeat("x", 13<<20)
	write := map[string]any{"kind": "session", "baseRevision": 0, "epoch": 1, "configRevision": 1, "generation": lease["generation"], "payload": payload}
	if status, body := h.req("PUT", "/v1/objects/"+objectA, token, write); status != 200 {
		t.Fatalf("supported large upload rejected: %d %v", status, body)
	}
	if status, body := h.req("GET", "/v1/objects/"+objectA+"?kind=session", token, nil); status != 200 || body["payload"] != payload {
		t.Fatalf("large download changed: %d", status)
	}
	write["payload"] = "stale replacement"
	if status, _ := h.req("PUT", "/v1/objects/"+objectA, token, write); status != 409 {
		t.Fatal("stale revision accepted", status)
	}
	if _, body := h.req("GET", "/v1/objects/"+objectA+"?kind=session", token, nil); body["payload"] != payload {
		t.Fatal("rejected write changed the snapshot")
	}
	h.login("large-snapshot@example.com", deviceB)
	key := ed25519.NewKeyFromSeed(bytes.Repeat([]byte{9}, 32))
	rotation := map[string]any{"baseEpoch": 1, "revokeDevice": deviceB, "recoveryEnvelope": "new-recovery-envelope", "recoveryVerifier": base64.StdEncoding.EncodeToString(key.Public().(ed25519.PublicKey)), "devices": []any{map[string]any{"id": deviceA, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "new-device-envelope"}}, "objects": []any{map[string]any{"id": objectA, "kind": "config", "baseRevision": 1, "payload": "config-rotated", "deleted": false, "generation": 0}, map[string]any{"id": objectA, "kind": "session", "baseRevision": 1, "payload": payload, "deleted": false, "generation": lease["generation"]}}}
	config := rotation["objects"].([]any)[0].(map[string]any)
	config["payload"] = strings.Repeat("c", 13<<20)
	if status, _ := h.req("POST", "/v1/vault/rotate", token, rotation); status != 409 {
		t.Fatal("oversize configuration accepted during rotation", status)
	}
	if _, body := h.req("GET", "/v1/objects/"+objectA+"?kind=session", token, nil); body["payload"] != payload || body["epoch"] != float64(1) || body["revision"] != float64(1) {
		t.Fatal("rejected rotation changed the original snapshot")
	}
	config["payload"] = "config-rotated"
	if status, body := h.req("POST", "/v1/vault/rotate", token, rotation); status != 200 {
		t.Fatalf("large rotation failed: %d %v", status, body)
	}
	if _, body := h.req("GET", "/v1/objects/"+objectA+"?kind=session", token, nil); body["payload"] != payload || body["epoch"] != float64(2) {
		t.Fatal("rotation changed large payload")
	}
}
func TestLargeSharePreservesConsentAndCompletePayload(t *testing.T) {
	h, owner, friend, id := sharingUsers(t)
	payload := strings.Repeat("s", 13<<20)
	body := sharingBody(id, time.Now().Add(time.Hour))
	body["payload"] = payload
	body["includesSession"] = true
	if status, out := h.req("POST", "/v1/shares", owner, body); status != 201 {
		t.Fatalf("large share rejected: %d %v", status, out)
	}
	if status, _ := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); status != 409 {
		t.Fatal("payload leaked before consent")
	}
	if status, _ := h.req("POST", "/v1/shares/"+shareA+"/accept", friend, nil); status != 200 {
		t.Fatal(status)
	}
	if status, out := h.req("GET", "/v1/shares/"+shareA+"/payload", friend, nil); status != 200 || out["payload"] != payload {
		t.Fatal("large shared payload changed", status)
	}
}
func TestSnapshotTransfersBoundSizeAndConcurrentMaterialization(t *testing.T) {
	request := httptest.NewRequest("PUT", "/v1/objects/example", nil)
	request.ContentLength = snapshotWireLimit + 1
	response := httptest.NewRecorder()
	if _, ok := beginSnapshotTransfer(response, request, snapshotWireLimit); ok || response.Code != 413 {
		t.Fatal("oversize declared body accepted")
	}
	request.ContentLength = 13 << 20
	first, ok := beginSnapshotTransfer(httptest.NewRecorder(), request, snapshotWireLimit)
	if !ok {
		t.Fatal("first slot unavailable")
	}
	defer first()
	second, ok := beginSnapshotTransfer(httptest.NewRecorder(), request, snapshotWireLimit)
	if !ok {
		t.Fatal("second slot unavailable")
	}
	defer second()
	response = httptest.NewRecorder()
	if _, ok := beginSnapshotTransfer(response, request, snapshotWireLimit); ok || response.Code != 429 {
		t.Fatal("unbounded concurrent snapshot admitted")
	}
	request.ContentLength = 1024
	release, ok := beginSnapshotTransfer(httptest.NewRecorder(), request, snapshotWireLimit)
	if !ok {
		t.Fatal("small writes must not be blocked by large transfers")
	}
	release()
}
