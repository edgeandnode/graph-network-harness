# Execution Targets Analysis

## Overview
Services in the harness can be executed in many different ways, each with different lifecycle management implications.

## Execution Target Types

### 1. Raw Command
- One-off execution
- No lifecycle management
- Example: `ls -la`, `curl http://localhost:8080`

### 2. Managed Process
- Harness manages PID files, restarts, logs
- Direct control over process lifecycle
- Example: `./bin/api-server --port 8080`

### 3. Systemd Service
- System service manager handles lifecycle
- Can be on local or remote hosts
- Example: `systemctl start postgresql`

### 4. Systemd-Portable Service
- Portable service images
- Can attach/detach dynamically
- Example: `portablectl attach myservice.portable`

### 5. Docker Container (Standalone)
- Individual container lifecycle
- Direct docker commands
- Example: `docker run -d postgres:14`

### 6. Docker Compose Service
- Part of a compose stack
- Shared networks, volumes, dependencies
- Example: `docker-compose up -d postgres`

### 7. Docker Compose Stack
- Entire stack of services
- Managed as a unit
- Example: `docker-compose up -d`

## Service Registry Implications

The service registry needs to track:

### 1. Service Identity
```yaml
service:
  name: postgres
  execution_type: compose_service
  location: local
  compose_file: ./docker-compose.yml
  compose_service_name: postgres
```

### 2. Network Information
Different execution types expose services differently:

- **Managed Process**: Direct host IP + port
- **Systemd Service**: Host IP + well-known port
- **Docker Container**: Container IP or host port mapping
- **Compose Service**: Service name within compose network + internal port

### 3. Dependency Resolution
Compose services have special considerations:

```yaml
# If api-server depends on postgres:
api-server:
  requires: [postgres]
  
# But postgres is in compose, api-server is managed process
# Registry must know:
# - postgres is accessible at host:5432 (compose port mapping)
# - OR api-server needs to join compose network
```

### 4. Lifecycle State
```yaml
postgres:
  execution_type: compose_service
  state: running
  health: healthy
  # Compose-specific state
  compose_stack_state: up
  scale: 1
```

### 5. Multi-Service Coordination
When services span different execution types:

```yaml
# Compose stack provides postgres, redis
compose_stack:
  file: ./docker-compose.yml
  services:
    postgres: 
      internal_port: 5432
      external_port: 5432
    redis:
      internal_port: 6379
      external_port: 6379

# Managed process needs to connect
api-server:
  type: managed_process
  # Gets injected:
  # POSTGRES_ADDR=localhost:5432
  # REDIS_ADDR=localhost:6379

# Remote service needs different IPs
remote-service:
  type: managed_process
  location: remote
  # Gets injected:
  # POSTGRES_ADDR=192.168.1.50:5432  (harness host IP)
  # REDIS_ADDR=192.168.1.50:6379
```

## Design Considerations

1. **Compose Network Joining**: Should managed processes be able to join compose networks?
2. **Port Mapping Tracking**: Registry must track both internal and external ports
3. **Stack Dependencies**: Can we depend on individual compose services or only the whole stack?
4. **Restart Coordination**: How to handle when compose stack restarts?