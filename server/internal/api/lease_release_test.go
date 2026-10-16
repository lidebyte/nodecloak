package api

import (
	"bytes"
	"encoding/base64"
	"net/http"
	"testing"
)

func TestSessionLeaseReleaseAllowsImmediateDeviceHandoff(t *testing.T) {
	h := setup(t)
	a := h.login("handoff@example.com", deviceA)["accessToken"].(string)
	h.enroll(a)
	b := h.login("handoff@example.com", deviceB)["accessToken"].(string)
	_, devices := h.req("GET", "/v1/devices", a, nil)
	var fp string
	for _, d := range devices["devices"].([]any) {
		v := d.(map[string]any)
		if v["id"] == deviceB {
			fp = v["fingerprint"].(string)
		}
	}
	if status, _ := h.req("POST", "/v1/devices/"+deviceB+"/approve", a, map[string]any{"fingerprint": fp, "deviceProof": base64.StdEncoding.EncodeToString(bytes.Repeat([]byte{1}, 32)), "envelope": "ciphertext", "epoch": 1}); status != 200 {
		t.Fatal(status)
	}
	route := "/v1/objects/" + objectA + "/lease"
	_, first := h.req("POST", route, a, map[string]any{"takeover": false})
	if status, body := h.req(http.MethodDelete, route, b, map[string]any{"generation": first["generation"]}); status != 200 || body["released"] != false {
		t.Fatalf("foreign release: %d %v", status, body)
	}
	if status, _ := h.req("POST", route, b, map[string]any{"takeover": false}); status != 409 {
		t.Fatal("foreign release expired an active lease")
	}
	if status, body := h.req(http.MethodDelete, route, a, map[string]any{"generation": first["generation"]}); status != 200 || body["released"] != true {
		t.Fatalf("owner release: %d %v", status, body)
	}
	status, second := h.req("POST", route, b, map[string]any{"takeover": false})
	if status != 200 || second["generation"].(float64) <= first["generation"].(float64) {
		t.Fatalf("handoff reused the old fence: %d %v", status, second)
	}
	if status, body := h.req(http.MethodDelete, route, a, map[string]any{"generation": first["generation"]}); status != 200 || body["released"] != false {
		t.Fatalf("stale release: %d %v", status, body)
	}
	if status, _ := h.req("POST", route, a, map[string]any{"takeover": false}); status != 409 {
		t.Fatal("stale owner released the new device's lease")
	}
	// A delayed close notification from the same device must not expire its newer launch.
	if status, _ := h.req(http.MethodDelete, route, b, map[string]any{"generation": second["generation"]}); status != 200 {
		t.Fatal(status)
	}
	_, third := h.req("POST", route, b, map[string]any{"takeover": false})
	h.req(http.MethodDelete, route, b, map[string]any{"generation": second["generation"]})
	if status, _ := h.req("POST", route, a, map[string]any{"takeover": false}); status != 409 {
		t.Fatal("stale same-device release expired a renewed fence")
	}
	if third["generation"].(float64) <= second["generation"].(float64) {
		t.Fatal("released fence reused")
	}
}

func TestNewLaunchAdvancesFenceButHeartbeatRetainsIt(t *testing.T) {
	h := setup(t)
	token := h.login("relaunch@example.com", deviceA)["accessToken"].(string)
	h.enroll(token)
	route := "/v1/objects/" + objectA + "/lease"
	_, first := h.req("POST", route, token, map[string]any{"takeover": false})
	_, renewed := h.req("POST", route, token, map[string]any{"takeover": false})
	if renewed["generation"] != first["generation"] {
		t.Fatal("heartbeat changed the active fence")
	}
	status, next := h.req("POST", route, token, map[string]any{"takeover": false, "newSession": true})
	if status != 200 {
		t.Fatalf("new launch: %d %v", status, next)
	}
	if next["generation"].(float64) <= first["generation"].(float64) {
		t.Fatal("new launch reused a delayed close's fence")
	}
	if status, body := h.req("DELETE", route, token, map[string]any{"generation": first["generation"]}); status != 200 || body["released"] != false {
		t.Fatalf("old close expired the new launch: %d %v", status, body)
	}
	if status, body := h.req("DELETE", route, token, map[string]any{"generation": next["generation"]}); status != 200 || body["released"] != true {
		t.Fatalf("new launch cannot release: %d %v", status, body)
	}
}
