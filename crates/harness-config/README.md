# harness-config

YAML configuration parser and resolver for the graph-network-harness.

## Overview

This crate provides parsing and resolution of `services.yaml` configuration files, converting them into strongly-typed configuration structures used by the service orchestrator.

## Features

- **YAML Parsing**: Parse service definitions from YAML files
- **Variable Resolution**: Environment variable substitution with `${VAR}` syntax
- **Service References**: Cross-service references with `${service.property}` syntax
- **Validation**: Configuration validation with clear error messages
- **Type Conversion**: Converts parsed config to orchestrator types

## Configuration Schema

```yaml
version: "1.0"

services:
  postgres:
    type: docker
    image: postgres:15
    env:
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    health_check:
      command: pg_isready
      interval: 30
      
  api:
    type: process
    binary: ./target/release/api
    env:
      DATABASE_URL: postgresql://postgres:${DB_PASSWORD}@${postgres.ip}:5432/db
    dependencies:
      - postgres
```

## Usage

```rust
use harness_config::{Config, parse_config};

// Parse configuration file
let config = parse_config("services.yaml").await?;

// Access resolved service configurations
for (name, service) in config.services {
    println!("Service: {} ({})", name, service.service_type);
}
```

## Variable Resolution

The resolver handles two types of substitutions:

1. **Environment Variables**: `${VAR_NAME}` - replaced with environment variable values
2. **Service References**: `${service.property}` - replaced with properties from other services (e.g., `${postgres.ip}`)

Resolution happens after parsing, ensuring all references are valid and circular dependencies are detected.

## Integration

This crate is used by the harness CLI to parse user configuration files and convert them into the internal types used by `service-orchestration`.