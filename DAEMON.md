# Harness Executor Daemon

The `harness-executor-daemon` is a persistent background service that manages service orchestration for the graph-network-harness. The CLI connects to this daemon via secure WebSocket over TLS.

## Architecture

- **Daemon**: Runs independently as `harness-executor-daemon` 
- **CLI**: Connects as a client via WebSocket on port 9443
- **Security**: Uses self-signed TLS certificates for secure communication
- **Data**: Stores state in `~/.local/share/harness/`

## Starting the Daemon

```bash
# Start the daemon (runs in foreground)
harness-executor-daemon

# The daemon will:
# - Bind to 127.0.0.1:9443 (TLS WebSocket)
# - Generate certificates if needed (1-year expiry)
# - Log to ~/.local/share/harness/daemon.log
# - Create persistent registry at ~/.local/share/harness/registry.json
```

## Certificate Management

The daemon uses self-signed TLS certificates stored in `~/.local/share/harness/certs/`:

- `server.crt` - Public certificate (trusted by CLI)
- `server.key` - Private key (daemon only)
- `README.md` - Certificate documentation

### Certificate Regeneration

```bash
# Regenerate expired certificates
harness-executor-daemon --regen-certs

# Check certificate expiry
harness daemon status
```

Certificates are automatically validated on startup and have a 1-year expiry.

## CLI Usage

```bash
# Check daemon status
harness daemon status

# The CLI will show connection status and certificate validity
```

## Data Directory Structure

```
~/.local/share/harness/
├── certs/
│   ├── server.crt       # TLS certificate
│   ├── server.key       # TLS private key
│   └── README.md        # Certificate info
├── daemon.log           # Daemon logs
└── registry.json        # Service registry state
```

## Network Security

- **Bind Address**: 127.0.0.1 (loopback only) 
- **Port**: 9443 (secure WebSocket)
- **Protocol**: WSS (WebSocket Secure)
- **Authentication**: Certificate-based trust

The daemon only accepts connections from localhost for security.

## Logging

- **File**: `~/.local/share/harness/daemon.log`
- **Format**: Structured JSON with timestamps
- **Rotation**: No automatic rotation (append-only)

View logs:
```bash
tail -f ~/.local/share/harness/daemon.log
```

## Service Management

The daemon manages services through:

- **Docker containers** via Docker API
- **Local processes** via process spawning  
- **Remote services** via SSH connections
- **Health checks** for service monitoring

All service state persists in the registry file.

## Development

Test the daemon with the included test script:
```bash
./test_daemon_tls.sh
```

This validates:
- Daemon startup and TLS binding
- CLI connection and certificate trust
- Multiple concurrent connections
- Proper shutdown handling