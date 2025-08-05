# Demo Daemon Example

This example demonstrates the harness daemon functionality with a WebSocket server for CLI-daemon communication.

## Purpose

The demo daemon provides:
- **WebSocket Server**: Secure TLS-enabled server on port 9443
- **CLI Integration**: Test environment for harness CLI commands
- **Service Management**: Example services (postgres, redis, api) for testing
- **Development Tool**: Interactive environment for experimenting with the harness

## Running the Demo

Start the demo daemon:
```bash
cargo run --example demo-daemon-ws --features smol
```

The daemon will:
1. Generate TLS certificates automatically (stored in `~/.local/share/harness/certs/`)
2. Start WebSocket server on `wss://127.0.0.1:9443`
3. Initialize the service registry
4. Display helpful commands to try

## Testing CLI Commands

In another terminal, try these commands:

### Check daemon status
```bash
harness daemon status
```

### List services
```bash
harness status
```

### Start a service
```bash
harness start postgres
```

### Stop a service
```bash
harness stop postgres
```

## Configuration

The demo uses the default `services.yaml` configuration file with example services:
- **postgres**: Database service
- **redis**: Cache service  
- **api**: Application service

## Architecture

The demo daemon demonstrates:
- **TLS Security**: Self-signed certificates for secure communication
- **Runtime Agnostic**: Uses smol for async execution
- **Service Orchestration**: Full harness orchestration capabilities
- **WebSocket Protocol**: JSON-based request/response over WebSocket
- **In-Memory State**: No persistence required for demo purposes

## Stopping the Daemon

Press `Ctrl+C` to gracefully stop the daemon.

## Files

- `demo-daemon-ws.rs` - Main demo daemon implementation
- `services.yaml` - Example service configuration (in project root)
- `~/.local/share/harness/` - Generated certificates and state directory