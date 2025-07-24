# Phase 5: Configuration & CLI - Implementation Plan

## Overview
Phase 5 adds user-facing configuration and command-line interface to make the orchestrator accessible without writing Rust code.

## Goals
1. Parse YAML service definitions
2. Provide intuitive CLI for service management
3. Support environment variable substitution
4. Enable service dependency resolution
5. Provide helpful error messages and validation

## Implementation Timeline

### Week 1: Configuration System (Jan 27-31)

#### Day 1-2: YAML Schema Design
- [ ] Define services.yaml schema (version, networks, services)
- [ ] Create serde structs for configuration
- [ ] Design network topology representation
- [ ] Plan environment variable substitution syntax

#### Day 3-4: Parser Implementation
- [ ] Implement YAML parser with serde_yaml
- [ ] Add environment variable substitution
- [ ] Implement configuration validation
- [ ] Create error types for configuration issues

#### Day 5: Testing
- [ ] Unit tests for parser
- [ ] Test invalid configurations
- [ ] Test environment variable substitution
- [ ] Integration with orchestrator types

### Week 2: CLI Structure (Feb 3-7)

#### Day 1-2: CLI Framework
- [ ] Create harness binary crate
- [ ] Set up clap for argument parsing
- [ ] Design command structure (init, start, stop, status, deploy, logs)
- [ ] Implement configuration file discovery

#### Day 3-4: Core Commands
- [ ] `harness init` - Initialize new project with example config
- [ ] `harness validate` - Validate services.yaml
- [ ] `harness list` - List available services
- [ ] `harness status` - Show service status

#### Day 5: Service Management
- [ ] `harness start [service]` - Start service(s)
- [ ] `harness stop [service]` - Stop service(s)
- [ ] `harness restart [service]` - Restart service(s)
- [ ] `harness logs [service]` - Stream service logs

### Week 3: Advanced Features (Feb 10-14)

#### Day 1-2: Dependency Resolution
- [ ] Implement topological sort for dependencies
- [ ] Add parallel startup for independent services
- [ ] Handle circular dependency detection
- [ ] Progress reporting during startup

#### Day 3-4: Deployment & Package Management
- [ ] `harness deploy` - Deploy packages to remote hosts
- [ ] `harness package` - Create deployment packages
- [ ] Progress bars for long operations
- [ ] Rollback support for failed deployments

#### Day 5: Polish & Testing
- [ ] Integration tests for all commands
- [ ] Error message improvements
- [ ] Help text and examples
- [ ] Shell completion scripts

## Technical Design

### Configuration Schema

```yaml
# services.yaml
version: "1.0"

# Global settings
settings:
  log_level: info
  health_check_interval: 30
  startup_timeout: 300

# Network topology definition
networks:
  local:
    type: local
    subnet: "127.0.0.0/8"
  
  lan:
    type: lan
    subnet: "192.168.1.0/24"
    discovery:
      method: "broadcast"
      port: 8888
  
  wireguard:
    type: wireguard
    subnet: "10.0.0.0/24"
    config_path: "/etc/wireguard/wg0.conf"

# Service definitions
services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${POSTGRES_PASSWORD}"
      POSTGRES_DB: "myapp"
    ports:
      - 5432
    volumes:
      - "${DATA_DIR}/postgres:/var/lib/postgresql/data"
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5
      timeout: 5

  api:
    type: process
    network: lan
    host: "192.168.1.100"
    binary: "/opt/api/bin/server"
    args:
      - "--port=8080"
      - "--workers=4"
    env:
      DATABASE_URL: "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/myapp"
      LOG_LEVEL: "${LOG_LEVEL:-info}"
    dependencies:
      - postgres
    health_check:
      http: "http://localhost:8080/health"
      interval: 30
      retries: 3

  worker:
    type: package
    network: wireguard
    host: "10.0.0.10"
    package: "./packages/worker-${VERSION}.tar.gz"
    env:
      API_URL: "http://${api.ip}:8080"
      WORKER_ID: "${HOSTNAME}"
    dependencies:
      - api
```

### CLI Command Structure

```bash
# Initialize new project
harness init [--example graph-node|microservices|distributed]

# Validate configuration
harness validate [--file services.yaml]

# Service management
harness start [service...] [--all] [--parallel]
harness stop [service...] [--all] [--force]
harness restart [service...]
harness status [service...] [--watch]

# Logs and debugging
harness logs <service> [--follow] [--tail 100]
harness exec <service> -- <command>
harness health <service>

# Deployment
harness deploy <service> [--version X.Y.Z]
harness package <directory> --output <package.tar.gz>
harness rollback <service>

# Network management
harness network list
harness network discover
harness network test <service1> <service2>

# Configuration
harness config get <key>
harness config set <key> <value>
harness config env
```

### Error Handling

All errors should be user-friendly:

```
Error: Failed to start service 'api'
  Caused by: Dependency 'postgres' is not healthy
  
  Try: harness logs postgres
       harness health postgres
```

### Progress Reporting

Use indicatif for progress bars:

```
Starting services...
  ✓ postgres (healthy in 3.2s)
  ✓ redis (healthy in 0.8s)
  ⠸ api (waiting for dependencies...)
  - worker (pending)
```

## Testing Strategy

1. **Unit Tests**
   - Configuration parsing
   - Dependency resolution
   - Command parsing

2. **Integration Tests**
   - Full CLI commands with test configs
   - Multi-service scenarios
   - Error cases and recovery

3. **End-to-End Tests**
   - Real service deployments
   - Cross-network communication
   - Package deployment flows

## Definition of Done

- [ ] All commands implemented and tested
- [ ] Comprehensive help text and examples
- [ ] Shell completion for bash/zsh/fish
- [ ] User guide documentation
- [ ] 90%+ test coverage
- [ ] No unwraps in production code
- [ ] All errors have helpful messages
- [ ] Performance: <100ms for status checks