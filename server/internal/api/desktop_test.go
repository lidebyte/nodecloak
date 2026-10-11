package api

import (
	"context"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// This mailbox exists exclusively in a Go test binary, never in the service.
type fileTestMailer struct{ dir string }

func (m fileTestMailer) Send(_ context.Context, _ string, code string) error {
	return os.WriteFile(filepath.Join(m.dir, "code"), []byte(code), 0600)
}
func TestDesktopInteropHarness(t *testing.T) {
	dir := os.Getenv("DESKTOP_INTEROP_DIR")
	if dir == "" {
		t.Skip("desktop interop harness not requested")
	}
	h := setup(t)
	h.s.mail = fileTestMailer{dir}
	server := httptest.NewServer(h.s.Handler())
	defer server.Close()
	if e := os.WriteFile(filepath.Join(dir, "endpoint"), []byte(server.URL), 0600); e != nil {
		t.Fatal(e)
	}
	deadline := time.Now().Add(10 * time.Minute)
	for time.Now().Before(deadline) {
		if _, e := os.Stat(filepath.Join(dir, "done")); e == nil {
			return
		}
		time.Sleep(200 * time.Millisecond)
	}
	t.Fatal("desktop harness timed out")
}
