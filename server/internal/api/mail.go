package api

import (
	"context"
	"errors"
	gomail "github.com/wneessen/go-mail"
	"time"
)

type SMTPConfig struct {
	Host                     string
	Port                     int
	Username, Password, From string
}
type SMTPMailer struct{ cfg SMTPConfig }

func NewSMTPMailer(c SMTPConfig) (Mailer, error) {
	if c.Host == "" || c.Port < 1 || c.Port > 65535 || c.From == "" || c.Username == "" || c.Password == "" {
		return nil, errors.New("SMTP configuration incomplete")
	}
	return &SMTPMailer{c}, nil
}
func (m *SMTPMailer) Send(ctx context.Context, to, code string) error {
	msg := gomail.NewMsg()
	if e := msg.From(m.cfg.From); e != nil {
		return e
	}
	if e := msg.To(to); e != nil {
		return e
	}
	msg.Subject("NodeCloak sign-in code")
	msg.SetBodyString(gomail.TypeTextPlain, "Your NodeCloak sign-in code is "+code+". It expires in 10 minutes. Do not share it.")
	options := []gomail.Option{gomail.WithPort(m.cfg.Port), gomail.WithSMTPAuth(gomail.SMTPAuthPlain), gomail.WithUsername(m.cfg.Username), gomail.WithPassword(m.cfg.Password), gomail.WithTLSPolicy(gomail.TLSMandatory), gomail.WithTimeout(20 * time.Second)}
	if m.cfg.Port == 465 {
		options = append(options, gomail.WithSSL())
	}
	c, e := gomail.NewClient(m.cfg.Host, options...)
	if e != nil {
		return e
	}
	return c.DialAndSendWithContext(ctx, msg)
}
