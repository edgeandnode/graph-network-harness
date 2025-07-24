# Phase 5 MVP: Configuration System

## Objective

Implement the minimum viable configuration system that allows users to define and run services using YAML instead of Rust code.

## Core Features (2 weeks)

### Week 1: Configuration Parser

#### Day 1-2: Setup & Types
```rust
// crates/harness-config/src/lib.rs
pub struct Config {
    version: String,
    services: HashMap<String, Service>,
    networks: HashMap<String, Network>,
}

pub struct Service {
    pub service_type: ServiceType,
    pub network: String,
    pub env: HashMap<String, String>,
    pub dependencies: Vec<String>,
    pub health_check: Option<HealthCheck>,
}

pub enum ServiceType {
    Process { binary: String, args: Vec<String> },
    Docker { image: String, ports: Vec<u16> },
    Remote { host: String, binary: String },
}
```

#### Day 3-4: Parser Implementation
- [ ] YAML parsing with serde_yaml
- [ ] Environment variable substitution (${VAR} syntax)
- [ ] Service reference resolution (${service.ip})
- [ ] Basic validation

#### Day 5: Testing
- [ ] Unit tests for all service types
- [ ] Environment variable substitution tests
- [ ] Error case testing
- [ ] Example configurations

### Week 2: Minimal CLI

#### Day 1-2: CLI Structure
```bash
harness validate                    # Validate services.yaml
harness start [service...]         # Start service(s)
harness stop [service...]          # Stop service(s)  
harness status                     # Show status
```

#### Day 3-4: Integration
- [ ] Connect config parser to orchestrator
- [ ] Implement start command
- [ ] Implement stop command
- [ ] Implement status command

#### Day 5: Polish
- [ ] Error handling
- [ ] Basic help text
- [ ] Integration tests
- [ ] README updates

## What We're NOT Doing (Yet)

### Defer to Phase 6:
- Interactive prompts
- Progress bars
- Fancy error messages
- Shell completions
- Most subcommands (logs, exec, deploy, etc.)
- Network commands
- Config commands beyond validate

### Defer to Phase 7:
- Auto-scaling
- Monitoring integration
- Network state recovery
- Web UI

## Example Usage (Phase 5 MVP)

```yaml
# services.yaml
version: "1.0"

services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
    ports: [5432]
    health_check:
      command: "pg_isready"

  api:
    type: process
    network: local  
    binary: "./target/release/api"
    env:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD}@${postgres.ip}:5432/db"
    dependencies: ["postgres"]
```

```bash
# Validate configuration
$ harness validate
✓ Configuration valid

# Start services
$ harness start
Starting postgres... done
Starting api... done

# Check status
$ harness status
SERVICE   STATUS    HEALTH
postgres  running   healthy
api       running   healthy

# Stop services
$ harness stop
Stopping api... done
Stopping postgres... done
```

## Success Criteria

1. **It Works** - Can start/stop services defined in YAML
2. **It's Simple** - Minimal commands, clear output
3. **It's Testable** - Good test coverage
4. **It's Documented** - Clear examples and usage

## Implementation Checklist

### Configuration Parser
- [ ] Crate setup
- [ ] Type definitions  
- [ ] YAML parser
- [ ] Variable substitution
- [ ] Validation
- [ ] Tests

### Minimal CLI
- [ ] Crate setup
- [ ] Command structure
- [ ] Validate command
- [ ] Start command
- [ ] Stop command
- [ ] Status command
- [ ] Integration tests

### Documentation
- [ ] Update README with YAML examples
- [ ] Remove Rust code examples
- [ ] Add quickstart guide
- [ ] Document configuration schema

## Timeline

- **Week 1**: Configuration parser complete with tests
- **Week 2**: CLI commands working end-to-end
- **Result**: Users can use YAML instead of Rust

This focused scope ensures we deliver value quickly while setting up the foundation for Phase 6's enhanced CLI experience.