# Phase 5: CLI Design

## Overview

The harness CLI provides an intuitive interface for managing heterogeneous services across local, Docker, and remote environments.

## Design Principles

1. **Intuitive**: Common operations should be obvious
2. **Discoverable**: Help and examples readily available  
3. **Composable**: Commands work well with Unix pipes
4. **Responsive**: Immediate feedback for all operations
5. **Robust**: Graceful error handling and recovery

## Command Structure

### Global Options

```bash
harness [global-options] <command> [command-options] [arguments]

Global Options:
  -c, --config <file>      Config file (default: services.yaml)
  -e, --env <key=value>    Set environment variable
  -v, --verbose            Increase verbosity (-vvv for max)
  -q, --quiet              Suppress non-error output
  --no-color               Disable colored output
  --json                   Output JSON for scripting
  --timeout <seconds>      Global operation timeout
  --working-dir <path>     Change working directory
  --help                   Show help
  --version               Show version
```

### Core Commands

#### Service Management

```bash
# Start services
harness start [services...] [options]
  -a, --all                Start all services
  -d, --dependencies       Include dependencies (default)
  --no-dependencies        Don't start dependencies
  -p, --parallel <n>       Start up to N services in parallel
  --timeout <seconds>      Override startup timeout per service
  --wait                   Wait for services to be healthy (default)
  --no-wait                Return immediately after starting
  --force                  Try to start even if health checks fail
  
Examples:
  harness start                    # Start all services
  harness start api worker         # Start specific services
  harness start -d api             # Start api and its dependencies
  harness start --parallel 4       # Start services 4 at a time
  harness start api --timeout 600  # Allow 10 minutes for api startup
  harness start --no-wait          # Don't wait for health checks

# Stop services  
harness stop [services...] [options]
  -a, --all                Stop all services
  -f, --force              Force stop (SIGKILL)
  -t, --timeout <seconds>  Graceful shutdown timeout
  --no-dependencies        Don't stop dependent services
  
Examples:
  harness stop                     # Stop all services
  harness stop api                 # Stop api (dependents will fail)
  harness stop --force worker      # Force kill worker

# Restart services
harness restart [services...] [options]
  -a, --all                Restart all services
  -r, --rolling            Rolling restart (one at a time)
  --delay <seconds>        Delay between restarts
  
Examples:
  harness restart api              # Quick restart
  harness restart -r --delay 5     # Rolling restart with delay

# Service status
harness status [services...] [options]
  -a, --all                Show all services (default)
  -w, --watch              Watch mode (live updates)
  --no-health-check        Skip health checks (just show state)
  --dependencies           Show dependency graph
  -v, --verbose            Show detailed error messages
  
Examples:
  harness status                   # Show all services with health checks
  harness status -w                # Live status updates
  harness status --dependencies    # Show dependency tree
  harness status -v                # Show why services are unhealthy
```

#### Logs and Debugging

```bash
# View logs
harness logs <service> [options]
  -f, --follow             Follow log output
  -t, --tail <n>           Show last N lines
  --since <time>           Show logs since time
  --until <time>           Show logs until time
  --grep <pattern>         Filter logs by pattern
  --json                   Output as JSON lines
  
Examples:
  harness logs api -f              # Follow api logs
  harness logs worker --tail 100   # Last 100 lines
  harness logs api --since 1h      # Logs from last hour
  harness logs api --grep ERROR    # Only error lines

# Execute commands
harness exec <service> -- <command> [args...]
  -i, --interactive        Interactive mode (attach stdin)
  -t, --tty               Allocate pseudo-TTY
  --user <user>           Run as specific user
  --working-dir <path>    Working directory
  
Examples:
  harness exec api -- ps aux       # List processes
  harness exec postgres -- psql    # Interactive psql
  harness exec worker -- /bin/bash # Shell access

# Health checks
harness health [service] [options]
  -a, --all               Check all services
  -v, --verbose           Show check details
  --run-now              Force immediate check
  
Examples:
  harness health                   # All services health
  harness health api -v            # Detailed api health
  harness health --run-now         # Force health check
```

#### Configuration

```bash
# Initialize project
harness init [options]
  --name <name>           Project name
  --template <template>   Use template (graph-node, microservices)
  --force                Overwrite existing files
  
Examples:
  harness init                     # Interactive init
  harness init --template graph    # Graph node template

# Validate configuration
harness validate [options]
  --file <file>          Config file to validate
  --strict               Strict validation
  --show-schema          Show YAML schema
  
Examples:
  harness validate                 # Validate services.yaml
  harness validate --strict        # Include warnings

# List services
harness list [options]
  --format <format>      Output format (table, json, yaml)
  --filter <filter>      Filter expression
  --show-config         Show configuration
  
Examples:
  harness list                     # Table of services
  harness list --format json       # JSON output
  harness list --filter "type=docker"  # Only Docker services

# Configuration inspection
harness config [subcommand] [options]

Subcommands:
  show [service]         Show configuration (all or specific service)
  resolve [service]      Show config with variables resolved  
  deps [service]         Show dependency graph
  env [service]          List environment variables
  check                  Validate configuration
  
Options:
  --format <format>      Output format (yaml, json, table)
  
Examples:
  harness config show                      # Show entire config
  harness config show api                  # Show api service config
  harness config resolve api               # Show api with vars resolved
  harness config deps                      # Show all dependencies
  harness config deps api                  # Show what api depends on
  harness config env                       # List all service env vars
  harness config env api                   # List api's env vars
  harness config check                     # Validate config
```

#### Deployment

```bash
# Deploy packages
harness deploy <service> [options]
  --version <version>     Package version
  --target <host>        Override target host
  --dry-run             Show what would be done
  --rollback-on-failure  Auto rollback if unhealthy
  
Examples:
  harness deploy worker            # Deploy latest
  harness deploy worker --version 1.2.3
  harness deploy api --dry-run     # Preview deployment

# Create packages
harness package <directory> [options]
  -o, --output <file>     Output package file
  --include <pattern>     Include files matching pattern
  --exclude <pattern>     Exclude files matching pattern
  --compress             Compression level (0-9)
  
Examples:
  harness package ./api -o api.tar.gz
  harness package . --exclude "*.log"

# Rollback deployments  
harness rollback <service> [options]
  --to-version <version>  Rollback to specific version
  --last                 Rollback to previous version
  
Examples:
  harness rollback worker          # Previous version
  harness rollback api --to-version 1.2.0
```

#### Network Management

```bash
# Network information
harness network [command] [options]

harness network list              # List all networks
harness network show <network>    # Show network details  
harness network discover          # Discover services
harness network test <src> <dst>  # Test connectivity

Examples:
  harness network list
  harness network show lan
  harness network test api worker
```

#### Configuration Validation Details

```bash
# The config check command validates:

1. Structure & Syntax:
   - Valid YAML syntax
   - Required fields present (name, type, network)
   - Service types are valid (process, docker, package, ssh)
   - Version compatibility

2. References:
   - All service dependencies exist
   - All network references exist  
   - SSH hosts are defined in network nodes
   - Docker images are specified

3. Network & Connectivity:
   - No port conflicts on same host
   - Networks have valid subnet ranges
   - Service placement matches network type
   - Remote services have SSH credentials

4. Dependencies:
   - No circular dependencies
   - Dependencies form valid DAG
   - Cross-network dependencies are routable

5. Resources & Files:
   - Binary paths exist (for local processes)
   - SSH keys exist and have correct permissions
   - Package files exist (for deployments)
   - Docker volumes paths exist

6. Health Checks:
   - Health check commands are valid
   - HTTP health URLs are well-formed
   - Timeouts and intervals are reasonable

Examples:
  harness config check              # Full validation
  harness config check -v           # Verbose with warnings
  
  # Example output:
  Checking configuration...
  ✓ Valid YAML structure
  ✓ All required fields present
  ✓ No dependency cycles (4 services)
  ✓ All network references valid (3 networks)
  ✗ Port conflict: 'api' and 'web' both use port 8080 on localhost
  ✗ SSH key not found: /home/user/.ssh/deploy_key
  ⚠ Warning: Binary not found: /opt/worker/bin/worker (will fail if run locally)
  ⚠ Warning: Large health check interval for 'database': 300s
  
  2 errors, 2 warnings found
```

#### Environment Variable Listing

```bash
# List environment variables by service
harness config env [service]

Examples:
  # List all services and their env vars
  harness config env
  
  Service: postgres
    POSTGRES_PASSWORD = "${POSTGRES_PASSWORD}"
    POSTGRES_DB = "myapp"
  
  Service: api  
    DATABASE_URL = "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/myapp"
    API_PORT = "8080"
    LOG_LEVEL = "${LOG_LEVEL:-info}"
    WORKERS = "${WORKERS:-4}"
  
  Service: worker
    API_URL = "http://${api.ip}:${API_PORT}"
    WORKER_ID = "${HOSTNAME}"
  
  # List just one service
  harness config env api
  
  DATABASE_URL = "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/myapp"
  API_PORT = "8080"  
  LOG_LEVEL = "${LOG_LEVEL:-info}"
  WORKERS = "${WORKERS:-4}"
  
  # With resolved values
  harness config env api --resolved
  
  DATABASE_URL = "postgresql://postgres:secret123@192.168.1.10:5432/myapp"
  API_PORT = "8080"
  LOG_LEVEL = "info"  # (default)
  WORKERS = "4"      # (default)
```

### Interactive Features

#### Progress Indicators

```
Starting services... 
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ✓ redis       [============================] 100% (healthy in 0.8s)  
  ⠸ api         [==============>             ] 52% (waiting for health check, 156s/300s)
  - worker      [                            ] 0% (pending)
  
Overall: 2/4 services started

# If startup timeout is exceeded:
Starting services...
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ✗ api         [============================] 100% (timeout after 300s)
  
Error: Service 'api' failed to become healthy within 300 seconds
  
The service is still running but not healthy. You can:
  • Check logs: harness logs api --tail 100
  • Increase timeout: harness start api --timeout 600
  • Force start: harness start api --force
  • Stop service: harness stop api
```

#### Status Display

```
┌─────────────┬─────────┬────────────┬─────────┬───────────────┬──────────────┐
│ Service     │ Status  │ Health     │ Network │ Host          │ Uptime       │
├─────────────┼─────────┼────────────┼─────────┼───────────────┼──────────────┤
│ postgres    │ running │ ✓ healthy  │ local   │ localhost     │ 2h 15m       │
│ redis       │ running │ ✓ healthy  │ local   │ localhost     │ 2h 15m       │
│ api         │ running │ ✗ timeout  │ lan     │ 192.168.1.100 │ 1h 48m       │
│ worker      │ running │ ✗ no route │ wg      │ 10.0.0.10     │ 1h 48m       │
│ frontend    │ stopped │ -          │ lan     │ 192.168.1.101 │ -            │
└─────────────┴─────────┴────────────┴─────────┴───────────────┴──────────────┘

⚠ 2 services are unhealthy

Problems detected:
  • api: Health check timeout (possible network issue)
  • worker: No route to host 10.0.0.10 (VPN down?)

Try: harness restart api worker
```

#### Error Messages

```
Error: Failed to start service 'api'

  Caused by:
    → Dependency 'postgres' is not healthy
    → Health check failed after 5 retries
    → Connection refused: localhost:5432

  Possible solutions:
    • Check postgres logs: harness logs postgres --tail 50
    • Verify postgres configuration in services.yaml
    • Ensure port 5432 is not already in use
    • Try restarting postgres: harness restart postgres

  For more details, run with -v flag
```

### Shell Completion

Generate completion scripts:

```bash
# Bash
harness completion bash > /etc/bash_completion.d/harness

# Zsh  
harness completion zsh > ~/.zsh/completions/_harness

# Fish
harness completion fish > ~/.config/fish/completions/harness.fish

# PowerShell
harness completion powershell > harness.ps1
```

### Configuration Discovery

The CLI searches for configuration in order:

1. `--config` flag
2. `HARNESS_CONFIG` environment variable
3. `./services.yaml`
4. `./harness.yaml`
5. `./.harness/services.yaml`
6. `~/.config/harness/services.yaml`

### Environment Variables

```bash
HARNESS_CONFIG=/path/to/services.yaml
HARNESS_LOG_LEVEL=debug
HARNESS_NO_COLOR=1
HARNESS_TIMEOUT=300
```

### Output Formats

#### Default (Human-Readable)

```
Service Status:
  ✓ postgres (running, healthy)
  ✓ api (running, healthy)  
  ✗ worker (stopped)
```

#### JSON Output (--json)

```json
{
  "services": [
    {
      "name": "postgres",
      "status": "running",
      "health": "healthy",
      "uptime": 8100
    }
  ]
}
```

#### Quiet Mode (-q)

Only errors are shown, exit codes indicate success/failure.

### Exit Codes

- `0`: Success
- `1`: General error
- `2`: Configuration error
- `3`: Service error
- `4`: Network error
- `5`: Timeout
- `126`: Permission denied
- `127`: Command not found

## Implementation Notes

### Technology Stack

- **CLI Framework**: clap v4 with derive macros
- **Progress Bars**: indicatif
- **Tables**: comfy-table
- **Colors**: colored or termcolor
- **Async Runtime**: Tokio (CLI can use tokio)
- **Configuration**: serde + serde_yaml
- **Shell Completion**: clap_complete

### Error Handling

- All errors should have helpful messages
- Suggest commands to diagnose/fix issues
- Support `-v` for detailed error traces
- Log errors to file for post-mortem

### Performance

- Commands should start in <100ms
- Status checks should complete in <1s
- Use concurrent operations where possible
- Cache service information for quick access
