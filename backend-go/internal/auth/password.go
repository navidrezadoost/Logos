// Package auth — Argon2id password hashing compatible with Clojure's buddy-hashers.
//
// Clojure config (app.auth):
//   {:alg :argon2id :memory 32768 :iterations 3 :parallelism 2}
//
// PHC string format produced and consumed:
//   $argon2id$v=19$m=<mem>,t=<time>,p=<par>$<base64url-no-pad(salt)>$<base64url-no-pad(hash)>
//
// buddy-hashers (Bouncy Castle) uses the reference Argon2 encoding which is
// RFC 4648 base64 (standard alphabet A-Za-z0-9+/) WITHOUT padding.
// In Go that is base64.RawStdEncoding.
package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"fmt"
	"strings"

	"golang.org/x/crypto/argon2"
)

// Argon2id parameters matching Clojure's buddy-hashers defaults.
const (
	argonMemory      uint32 = 32768 // 32 MiB
	argonTimeCost    uint32 = 3
	argonParallelism uint8  = 2
	argonKeyLen      uint32 = 32
	argonSaltLen            = 16
)

// DerivePassword hashes password using Argon2id with the same parameters as
// Clojure's buddy-hashers, returning a PHC-format string.
func DerivePassword(password string) (string, error) {
	salt := make([]byte, argonSaltLen)
	if _, err := rand.Read(salt); err != nil {
		return "", fmt.Errorf("auth: generate salt: %w", err)
	}

	hash := argon2.IDKey(
		[]byte(password), salt,
		argonTimeCost, argonMemory, argonParallelism, argonKeyLen,
	)

	b64Salt := base64.RawStdEncoding.EncodeToString(salt)
	b64Hash := base64.RawStdEncoding.EncodeToString(hash)

	return fmt.Sprintf("$argon2id$v=19$m=%d,t=%d,p=%d$%s$%s",
		argonMemory, argonTimeCost, argonParallelism, b64Salt, b64Hash), nil
}

// VerifyPassword checks a plaintext password against a PHC-format Argon2id hash.
// It returns (valid=true, needsRehash=true) when the password is correct but
// the hash parameters differ from the current defaults (prompt a silent rehash).
func VerifyPassword(password, encoded string) (valid bool, needsRehash bool) {
	memory, timeCost, threads, keyLen, salt, expectedHash, err := parseArgon2PHC(encoded)
	if err != nil {
		return false, false
	}

	computed := argon2.IDKey([]byte(password), salt, timeCost, memory, threads, keyLen)
	valid = subtle.ConstantTimeCompare(computed, expectedHash) == 1
	needsRehash = valid && (memory != argonMemory || timeCost != argonTimeCost || threads != argonParallelism)
	return
}

// parseArgon2PHC parses a PHC-format Argon2id string.
// Format: $argon2id$v=<version>$m=<memory>,t=<time>,p=<parallelism>$<salt>$<hash>
func parseArgon2PHC(encoded string) (
	memory uint32, timeCost uint32, threads uint8, keyLen uint32,
	salt, hash []byte, err error,
) {
	// Split: ["", "argon2id", "v=19", "m=32768,t=3,p=2", "<salt>", "<hash>"]
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 {
		err = fmt.Errorf("auth: invalid argon2 hash: expected 6 '$'-separated parts, got %d", len(parts))
		return
	}
	if parts[1] != "argon2id" {
		err = fmt.Errorf("auth: unsupported algorithm %q (only argon2id supported)", parts[1])
		return
	}

	var version int
	if _, scanErr := fmt.Sscanf(parts[2], "v=%d", &version); scanErr != nil {
		err = fmt.Errorf("auth: parse version: %w", scanErr)
		return
	}

	var mem, t, p int
	if _, scanErr := fmt.Sscanf(parts[3], "m=%d,t=%d,p=%d", &mem, &t, &p); scanErr != nil {
		err = fmt.Errorf("auth: parse params: %w", scanErr)
		return
	}
	memory = uint32(mem)
	timeCost = uint32(t)
	threads = uint8(p)

	// Try raw (no-padding) first, fall back to padded for resilience.
	if salt, err = base64.RawStdEncoding.DecodeString(parts[4]); err != nil {
		if salt, err = base64.StdEncoding.DecodeString(parts[4]); err != nil {
			err = fmt.Errorf("auth: decode salt: %w", err)
			return
		}
	}

	if hash, err = base64.RawStdEncoding.DecodeString(parts[5]); err != nil {
		if hash, err = base64.StdEncoding.DecodeString(parts[5]); err != nil {
			err = fmt.Errorf("auth: decode hash: %w", err)
			return
		}
	}

	keyLen = uint32(len(hash))
	return
}
