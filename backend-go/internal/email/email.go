// Package email provides a minimal email sending interface.
//
// This implementation is a stub: all outbound emails are logged to stdout
// instead of being delivered via SMTP.  Replace Send with a real SMTP
// implementation (e.g. net/smtp or a transactional email provider) when needed.
package email

import (
	"fmt"
	"log"
)

// Kind identifies the email template to send.
type Kind string

const (
	KindRegister         Kind = "register"
	KindPasswordRecovery Kind = "password-recovery"
	KindEmailChange      Kind = "email-change"
)

// Message holds the data required to send one email.
type Message struct {
	Kind      Kind
	To        string
	Name      string
	Token     string
	PublicURI string
}

// Send logs the email details.  Swap this function for a real SMTP sender
// in production by replacing the body; the signature must stay the same.
func Send(msg Message) {
	log.Printf("[email] kind=%s to=%q name=%q link=%s/auth/%s?token=%s",
		msg.Kind, msg.To, msg.Name, msg.PublicURI, msg.Kind, msg.Token)
}

// VerifyLink builds the email-verification URL for convenience.
func VerifyLink(publicURI, token string) string {
	return fmt.Sprintf("%s/auth/verify-token?token=%s", publicURI, token)
}

// RecoveryLink builds the password-recovery URL for convenience.
func RecoveryLink(publicURI, token string) string {
	return fmt.Sprintf("%s/auth/recovery?token=%s", publicURI, token)
}
