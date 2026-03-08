# Future Work

Items deferred from the original SDD phased delivery plan.

## Deferred from Phase 4

- STARTTLS support (SMTP port 25/587, IMAP port 143, POP3 port 110) — requires plaintext protocol negotiation before TLS handshake upgrade
- Certificate expiry monitoring / scheduling (webhook on approaching expiry)
- TLS version/cipher trend tracking (compare scans over time)
- Batch inspection (`POST /api/inspect/batch` with multiple hostnames)
- Certificate diff between two hostnames

## Phase 5 — Not Committed

- ECH (Encrypted Client Hello) detection and testing
- Client certificate authentication testing
- mTLS inspection
- Certificate chain download (PEM/DER)
- Integration with CT monitor APIs (crt.sh, Certstream)
