# Harness TLS Certificates

These certificates secure communication between the harness CLI and executor daemon.

## Current Certificate
- Generated: 2025-08-05 03:10:40 UTC
- Expires: 2026-08-05 03:10:40 UTC
- Valid for: localhost, 127.0.0.1

## Certificate Details
- Algorithm: RSA 2048
- Self-signed: Yes
- Purpose: Development/Testing

## To Regenerate
Run: `harness daemon regenerate-certs`

## Using Custom Certificates
Replace server.crt and server.key with your own files.
The daemon will use whatever valid certificates are present.

## Security Note
These are self-signed certificates suitable for local development.
For production use, consider using certificates from a trusted CA.
