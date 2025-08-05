# Demo Services Example

This example demonstrates the harness capabilities by simulating a social media platform's microservices architecture.

## Overview

The demo includes:
- **Core Services**: Database, Cache, API Gateway, User Service, Feed Service, etc.
- **Background Workers**: Analytics, Media Processing
- **Tasks**: Database migrations, cache warming, asset deployment

## Running the Demo

### Current Implementation (Basic Demo)

```bash
# Run the demo daemon (runs for 10 seconds as a demonstration)
cargo run --example demo-daemon --features smol
```

### Future Implementation (Once WebSocket server is ready)

1. Start the demo services daemon:
```bash
cargo run --example demo-daemon --features smol
```

2. In another terminal, use the harness CLI to interact:
```bash
# Check daemon status
harness --daemon-port 8090 status

# Start services
harness --daemon-port 8090 start database
harness --daemon-port 8090 start api-gateway
harness --daemon-port 8090 start cache

# Run migration task
harness --daemon-port 8090 task run db-migration

# View logs
harness --daemon-port 8090 logs database --follow
```

## Service Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Database  │     │    Cache    │     │  API Gateway│
│  (Process)  │     │  (Process)  │     │   (Docker)  │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                    │
       └───────────┬───────┘                    │
                   │                            │
           ┌───────▼────────┐          ┌───────▼────────┐
           │  User Service  │          │  Feed Service  │
           │    (Docker)    │          │   (Process)    │
           └────────────────┘          └────────────────┘
```

## Features Demonstrated

1. **Dependency Management**: Services start in correct order
2. **Health Checks**: Each service has appropriate health checks
3. **Mixed Execution**: Both Docker and local process services
4. **Realistic Logging**: JSON logs, colored output, various log levels
5. **Failure Scenarios**: Some services occasionally fail and recover
6. **Task Execution**: One-time operations with progress tracking