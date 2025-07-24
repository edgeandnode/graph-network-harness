# Harness CLI

The harness CLI provides a simple interface for managing services defined in YAML configuration files.

## Installation

```bash
cargo install --path crates/harness
```

## Usage

### Basic Commands

```bash
# Validate configuration
harness validate                  # Basic validation
harness validate --strict         # Fail on missing environment variables

# Start all services
harness start                     # Shows progress indicators for each service

# Start specific services
harness start api worker          # Starts only specified services with dependencies

# Stop all services
harness stop                      # Stops in reverse dependency order
harness stop --force              # Continue despite errors
harness stop --timeout 30         # Wait up to 30 seconds for graceful shutdown

# Stop specific services
harness stop api                  # Warns about dependent services
harness stop api --force          # Force stop despite dependents

# Check service status
harness status                    # Basic table view
harness status --detailed         # Detailed view with network info
harness status --watch            # Real-time updates every 2 seconds
harness status --format json      # JSON output for automation
```

### Configuration File

By default, harness looks for `services.yaml` in the current directory. You can specify a different file with the `-c` flag:

```bash
harness -c my-services.yaml status
```

### Example Configuration

```yaml
version: "1.0"
name: "my-app"

networks:
  local:
    type: local

services:
  database:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD:-secret}"
    ports:
      - 5432
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  api:
    type: process
    network: local
    binary: "./api-server"
    env:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD:-secret}@${database.ip}:5432/myapp"
    dependencies:
      - database
```

## Features

### Environment Variable Substitution

- `${VAR}` - Use environment variable VAR (must be UPPERCASE)
- `${VAR:-default}` - Use VAR or "default" if not set
- `${service.ip}` - Reference another service's IP address
- `${service.port}` - Reference another service's port
- `${service.host}` - Reference another service's hostname

See [VARIABLE-SUBSTITUTION.md](../../VARIABLE-SUBSTITUTION.md) for complete documentation.

### Service Types

- **docker**: Run Docker containers
- **process**: Run local processes
- **remote**: Run processes on remote machines (via SSH)
- **package**: Deploy packages to remote machines

### Health Checks

Services can define health checks that run periodically:

```yaml
health_check:
  command: "curl"
  args: ["-f", "http://localhost:8080/health"]
  interval: 30    # seconds
  retries: 3      # attempts before marking unhealthy
  timeout: 10     # seconds per check
```

## Phase 5 MVP Status

This is the MVP implementation for Phase 5. It includes:

- ✅ YAML configuration parsing
- ✅ Environment variable substitution
- ✅ Basic CLI commands (validate, start, stop, status)
- ✅ Service dependency resolution
- ✅ Integration with the orchestrator library

Coming in Phase 6:
- Progress indicators and better UI
- Additional commands (logs, exec, restart)
- Shell completions
- Interactive error recovery
- Network management commands