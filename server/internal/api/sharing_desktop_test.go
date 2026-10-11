package api

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

type desktopSharingMail struct{ directory string }

func (m desktopSharingMail) Send(_ context.Context, email, code string) error {
	digest := sha256.Sum256([]byte(strings.ToLower(strings.TrimSpace(email))))
	return os.WriteFile(filepath.Join(m.directory, hex.EncodeToString(digest[:])+".code"), []byte(code), 0600)
}

// This harness exercises the production routes without a production login bypass.
// It only runs against the explicitly named disposable sharing integration DB.
func TestSharingDesktopHarness(t *testing.T) {
	directory := os.Getenv("SHARING_INTEROP_DIR")
	if directory == "" {
		t.Skip("SHARING_INTEROP_DIR is required for desktop integration")
	}
	dsn := os.Getenv("TEST_DATABASE_URL")
	parsed, err := url.Parse(dsn)
	if err != nil || parsed.Hostname() != "127.0.0.1" || parsed.Path != "/nodecloak_sharing_test" {
		t.Fatal("desktop sharing harness requires the isolated localhost test database")
	}
	db, err := pgxpool.New(context.Background(), dsn)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	if err = Migrate(context.Background(), db); err != nil {
		t.Fatal(err)
	}
	if err = os.MkdirAll(directory, 0700); err != nil {
		t.Fatal(err)
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	endpoint := "http://" + listener.Addr().String()
	service := New(db, Config{PublicURL: endpoint, TokenPepper: strings.Repeat("d", 64)}, desktopSharingMail{directory})
	server := &http.Server{Handler: service.Handler(), ReadHeaderTimeout: 5 * time.Second}
	go func() { _ = server.Serve(listener) }()
	defer server.Close()
	if err = os.WriteFile(filepath.Join(directory, "endpoint"), []byte(endpoint), 0600); err != nil {
		t.Fatal(err)
	}
	fmt.Println("Desktop sharing harness ready on isolated localhost service")
	timer := time.NewTimer(12 * time.Minute)
	defer timer.Stop()
	ticker := time.NewTicker(250 * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-timer.C:
			t.Fatal("desktop harness timed out")
		case <-ticker.C:
			if _, err = os.Stat(filepath.Join(directory, "done")); err == nil {
				return
			}
		}
	}
}
