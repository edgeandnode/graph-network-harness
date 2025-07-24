# Phase 5: YAML Configuration Schema

## Overview

The services.yaml file defines the complete service topology, network configuration, and deployment settings for the harness.

## Schema Structure

### Top-Level Fields

```yaml
version: "1.0"              # Required: Schema version
name: "my-deployment"       # Optional: Deployment name
description: "..."          # Optional: Human-readable description

settings:                   # Optional: Global settings
  log_level: info
  health_check_interval: 30
  startup_timeout: 300      # Default startup timeout for all services
  shutdown_timeout: 30      # Default graceful shutdown timeout
  
networks:                   # Required: Network topology
  <network-name>: ...
  
services:                   # Required: Service definitions
  <service-name>: ...
  
templates:                  # Optional: Reusable service templates
  <template-name>: ...
```

### Network Definitions

Networks define the topology and connectivity options:

```yaml
networks:
  # Local network (same machine)
  local:
    type: local
    subnet: "127.0.0.0/8"              # Optional: IP allocation range
    
  # LAN network (local area network)
  lan:
    type: lan
    subnet: "192.168.1.0/24"           # Required for IP allocation
    discovery:                         # Optional: Service discovery
      method: broadcast                # broadcast | multicast | static
      port: 8888
      interface: eth0                  # Optional: Specific interface
    nodes:                            # Optional: Pre-defined nodes
      - host: "192.168.1.100"
        name: "worker-1"
        ssh_user: "ubuntu"
        ssh_key: "~/.ssh/id_rsa"
        
  # WireGuard network
  wireguard:
    type: wireguard
    subnet: "10.0.0.0/24"
    interface: "wg0"                   # Optional: Interface name
    config_path: "/etc/wireguard/wg0.conf"
    nodes:
      - host: "10.0.0.10"
        name: "remote-1"
        public_key: "..."              # Optional: For validation
        ssh_user: "admin"
        ssh_key: "~/.ssh/wg_key"
        package_deploy: true           # Enable package deployment
```

### Service Definitions

Services can be of different types with specific configuration:

#### Docker Services

```yaml
services:
  postgres:
    type: docker
    network: local                     # Which network to attach to
    image: "postgres:15"
    tag: "${POSTGRES_VERSION:-15}"     # Optional: Override tag
    command: ["custom", "command"]     # Optional: Override CMD
    entrypoint: ["custom"]             # Optional: Override ENTRYPOINT
    env:
      POSTGRES_PASSWORD: "${DB_PASS}"
      POSTGRES_DB: "myapp"
    env_file: ".env.postgres"          # Optional: Load from file
    ports:
      - "5432:5432"                    # host:container
      - 5433                           # container port only
    volumes:
      - "${DATA_DIR}/postgres:/var/lib/postgresql/data"
      - "./init.sql:/docker-entrypoint-initdb.d/init.sql:ro"
    labels:
      app: "myapp"
      component: "database"
    resources:                         # Optional: Resource limits
      memory: "2G"
      cpus: "2.0"
    restart: "unless-stopped"          # no | always | on-failure | unless-stopped
    health_check:
      test: ["CMD", "pg_isready", "-U", "postgres"]
      interval: 10
      timeout: 5
      retries: 5
      start_period: 30
```

#### Process Services

```yaml
services:
  api:
    type: process
    network: lan
    host: "192.168.1.100"             # Optional: Specific host
    binary: "/opt/api/bin/server"      # Required: Executable path
    args:
      - "--port=8080"
      - "--config=/etc/api/config.yaml"
    env:
      DATABASE_URL: "postgresql://user:pass@${postgres.ip}:5432/db"
      API_KEY: "${API_KEY}"
    env_file: ".env.api"
    working_dir: "/opt/api"
    user: "api-user"                   # Optional: Run as user
    group: "api-group"                 # Optional: Run as group
    dependencies:
      - postgres
      - redis
    startup_timeout: 300               # Optional: Max time to become healthy (seconds)
    shutdown_timeout: 30               # Optional: Graceful shutdown time (seconds)
    resources:
      max_memory: "4G"                 # Optional: Memory limit
      nice: 10                         # Optional: Process priority
    health_check:
      http:
        url: "http://localhost:8080/health"
        method: "GET"                  # GET | POST | HEAD
        headers:
          Authorization: "Bearer ${HEALTH_TOKEN}"
        expected_status: 200           # or range: "200-299"
        timeout: 10
      interval: 30
      retries: 3
      start_period: 60
```

#### Package Services

```yaml
services:
  worker:
    type: package
    network: wireguard
    host: "10.0.0.10"                  # Required: Target host
    package: "./packages/worker-${VERSION}.tar.gz"
    version: "${VERSION}"              # Optional: For tracking
    env:
      API_URL: "http://${api.ip}:8080"
      WORKER_ID: "${host.name}"        # Built-in variables
      WORKER_ENV: "${network.name}"
    install_path: "/opt/worker"        # Optional: Override default
    pre_install:                       # Optional: Commands before
      - "systemctl stop worker || true"
    post_install:                      # Optional: Commands after
      - "systemctl daemon-reload"
      - "systemctl enable worker"
    dependencies:
      - api
    health_check:
      command: "/opt/worker/bin/health-check"
      args: ["--verbose"]
      interval: 60
      retries: 5
```

#### SSH Services (attach to existing)

```yaml
services:
  existing-service:
    type: ssh
    network: lan
    host: "192.168.1.200"
    ssh_user: "monitor"
    ssh_key: "~/.ssh/monitor_key"
    attach:
      pid_file: "/var/run/service.pid"
      process_name: "my-service"       # Alternative to pid_file
    health_check:
      command: "systemctl is-active my-service"
      interval: 30
```

### Service Templates

Templates allow reusing common configurations:

```yaml
templates:
  base-api:
    type: process
    env:
      LOG_LEVEL: "${LOG_LEVEL:-info}"
      METRICS_PORT: "9090"
    health_check:
      http: "http://localhost:${METRICS_PORT}/metrics"
      interval: 30

services:
  api-1:
    extends: base-api
    network: lan
    host: "192.168.1.100"
    binary: "/opt/api/bin/server"
    env:
      SERVICE_NAME: "api-1"
      PORT: "8081"
      
  api-2:
    extends: base-api
    network: lan  
    host: "192.168.1.101"
    binary: "/opt/api/bin/server"
    env:
      SERVICE_NAME: "api-2"
      PORT: "8082"
```

### Variable Substitution

The following variables are available for substitution:

```yaml
# Environment variables (must be UPPERCASE)
${DB_PASSWORD}         # Environment variable
${API_KEY:-default}    # Environment variable with default

# Service references
${<name>.ip}          # Another service's IP address
${<name>.port}        # Another service's first exposed port
${<name>.host}        # Another service's hostname

# Examples:
${postgres.ip}        # PostgreSQL service IP
${redis.port}         # Redis service port
${api.host}           # API service hostname
```

Note: Additional built-in variables like ${service.name}, ${network.name}, etc. are planned for future phases.

### Health Check Types

```yaml
health_check:
  # HTTP/HTTPS check
  http:
    url: "http://localhost:8080/health"
    method: "GET"
    headers:
      Authorization: "Bearer token"
    expected_status: 200
    expected_body: "ok"               # Optional: Body contains
    timeout: 10
    
  # TCP port check
  tcp:
    port: 8080
    timeout: 5
    
  # Command execution
  command: "health-check"
  args: ["--json"]
  expected_exit: 0                    # Optional: Exit code
  expected_output: "healthy"          # Optional: Output contains
  
  # Multiple checks (all must pass)
  all:
    - tcp:
        port: 8080
    - http:
        url: "http://localhost:8080/ready"
        
  # Common settings
  interval: 30                        # Seconds between checks
  timeout: 10                         # Timeout per check
  retries: 3                          # Retries before unhealthy
  start_period: 60                    # Grace period before first check
  success_threshold: 1                # Consecutive successes before healthy
```

### Deployment Strategies

```yaml
services:
  api:
    deployment:
      strategy: "rolling"              # rolling | recreate | blue-green
      max_parallel: 2                  # For rolling updates
      health_check_between: true       # Check health between instances
      rollback_on_failure: true        # Auto rollback if unhealthy
      
  worker:
    deployment:
      strategy: "recreate"             # Stop all, then start all
      stop_timeout: 30                 # Graceful shutdown time
```

## Environment Variable Resolution

The harness supports two types of variable substitution:

### Environment Variables
- `${VAR}` - Must be UPPERCASE only, fails if not set
- `${VAR:-default}` - Uses default value if VAR is not set

### Service References  
- `${service.ip}` - Service's IP address
- `${service.port}` - Service's first exposed port
- `${service.host}` - Service's hostname

Variables are resolved in this order:
1. Service references (${service.ip}, ${service.port}, ${service.host})
2. Shell environment variables
3. Default values (${VAR:-default})

See [VARIABLE-SUBSTITUTION.md](../../VARIABLE-SUBSTITUTION.md) for detailed documentation.

## Validation Rules

1. **Service names**: Must be unique, alphanumeric + dash/underscore
2. **Dependencies**: Must form a DAG (no cycles)
3. **Networks**: Services can only reference defined networks
4. **Hosts**: Must be reachable for remote services
5. **Ports**: No conflicts on the same host
6. **Required fields**: All required fields must be present
7. **Variable references**: All ${} references must resolve

## Example: Complete Configuration

```yaml
version: "1.0"
name: "graph-indexer-cluster"
description: "Graph Protocol indexer with PostgreSQL and IPFS"

settings:
  log_level: "${LOG_LEVEL:-info}"
  health_check_interval: 30
  startup_timeout: 300

networks:
  local:
    type: local
    
  lan:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        name: "indexer-1"
        ssh_user: "ubuntu"
        ssh_key: "~/.ssh/id_rsa"

services:
  postgres:
    type: docker
    network: local
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${POSTGRES_PASSWORD}"
      POSTGRES_DB: "graph_node"
    ports:
      - 5432
    volumes:
      - "${DATA_DIR}/postgres:/var/lib/postgresql/data"
    health_check:
      command: "pg_isready"
      interval: 10
      retries: 5

  ipfs:
    type: docker
    network: local
    image: "ipfs/go-ipfs:latest"
    ports:
      - 5001
      - 8080
    volumes:
      - "${DATA_DIR}/ipfs:/data/ipfs"
    health_check:
      http: "http://localhost:5001/api/v0/id"
      interval: 30

  graph-node:
    type: process
    network: lan
    host: "192.168.1.100"
    binary: "/opt/graph-node/bin/graph-node"
    env:
      GRAPH_NODE_CONFIG: "/etc/graph-node/config.toml"
      postgres_url: "postgresql://postgres:${POSTGRES_PASSWORD}@${postgres.ip}:5432/graph_node"
      ipfs: "http://${ipfs.ip}:5001"
      ethereum: "${ETHEREUM_RPC_URL}"
      GRAPH_LOG: "info"
    dependencies:
      - postgres
      - ipfs
    health_check:
      http: "http://localhost:8000/"
      interval: 30
      retries: 3
```