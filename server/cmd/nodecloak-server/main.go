package main

import (
	"context"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/nodecloak/nodecloak/server/internal/api"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"
)

func main() {
	cfg := api.Config{PublicURL: os.Getenv("PUBLIC_URL"), TokenPepper: os.Getenv("TOKEN_PEPPER"), GoogleClientID: os.Getenv("GOOGLE_CLIENT_ID"), GoogleClientSecret: os.Getenv("GOOGLE_CLIENT_SECRET"), TrustedProxies: strings.Fields(strings.ReplaceAll(os.Getenv("TRUSTED_PROXIES"), ",", " "))}
	if e := cfg.Validate(); e != nil {
		log.Fatal(e)
	}
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()
	db, e := pgxpool.New(ctx, os.Getenv("DATABASE_URL"))
	if e != nil {
		log.Fatal("database configuration invalid")
	}
	defer db.Close()
	if e = api.Migrate(ctx, db); e != nil {
		log.Fatal("database migration failed")
	}

	if e = api.BootstrapAdmins(ctx, db, strings.Fields(strings.ReplaceAll(os.Getenv("ADMIN_EMAILS"), ",", " "))); e != nil {
		log.Fatal("administrator bootstrap failed")
	}
	var mail api.Mailer
	if os.Getenv("SMTP_HOST") != "" {
		port, _ := strconv.Atoi(os.Getenv("SMTP_PORT"))
		mail, e = api.NewSMTPMailer(api.SMTPConfig{Host: os.Getenv("SMTP_HOST"), Port: port, Username: os.Getenv("SMTP_USERNAME"), Password: os.Getenv("SMTP_PASSWORD"), From: os.Getenv("SMTP_FROM")})
		if e != nil {
			log.Fatal(e)
		}
	}
	service := api.New(db, cfg, mail)
	addr := os.Getenv("LISTEN_ADDR")
	if addr == "" {
		addr = "127.0.0.1:8088"
	}
	srv := &http.Server{Addr: addr, Handler: service.Handler(), ReadHeaderTimeout: 5 * time.Second, ReadTimeout: 30 * time.Second, WriteTimeout: 30 * time.Second, IdleTimeout: 60 * time.Second, MaxHeaderBytes: 16 << 10}
	go func() {
		ticker := time.NewTicker(time.Minute)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				if service.Cleanup(ctx) != nil {
					log.Print("ephemeral data cleanup failed")
				}
			}
		}
	}()
	go func() {
		<-ctx.Done()
		deadline, stop := context.WithTimeout(context.Background(), 10*time.Second)
		defer stop()
		_ = srv.Shutdown(deadline)
	}()
	log.Printf("NodeCloak account service listening on %s", addr)
	if e = srv.ListenAndServe(); e != nil && e != http.ErrServerClosed {
		log.Fatal("HTTP service stopped")
	}
}
