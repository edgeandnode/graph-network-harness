# ADR-007: Distributed Service Orchestration

## Status
Proposed

## Context

Traditional Docker-based test harnesses have significant limitations when used for integration testing and development. Container rebuilds are slow (30-60 seconds for small changes), debugging through container boundaries is complex, and Docker's bridge network doesn't extend to native processes or remote machines. Additionally, running everything in containers adds unnecessary resource overhead and makes it difficult to test distributed scenarios.

## Decision

Transform the test harness from a Docker-only system into a **heterogeneous service harness** that can run services anywhere (local processes, containers, remote machines) while making them all appear to be on the same network.

### Core Features

1. **Run Anywhere**: Services can execute as:
   - Local processes (for fast dev iteration)
   - Docker containers (for isolation)
   - LAN nodes requiring remote administration but no vpn is needed to reach them.
   - Remote processes via SSH (for distributed testing)

2. **Unified Networking**: All services can talk to each other regardless of where/how they run:
   - Direct LAN connections
   - Local processes
   - Local and remote docker containers
   - WireGuard mesh creates a virtual network spanning all locations

3. **Transparent Discovery**: Services find each other by name:
   - Automatically routes to correct IP based on caller location

4. **Zero-Config Deployment**: Package and deploy to remote machines:
   - Bundle service + config + setup script
   - Deploy via SSH with automatic network setup

## Implementation Details

### 1. Runtime Abstraction Layer

The harness provides a unified interface for managing services across different execution environments. Each runtime implementation must handle:

- **Process lifecycle management**: Start, stop, restart services
- **Log collection and streaming**: Capture stdout/stderr, follow logs in real-time
- **Health monitoring**: Check service status, implement health endpoints
- **Configuration injection**: Pass environment variables, mount config files
- **Network setup**: Configure service networking based on target type

Service locations include:
- **Local**: Process or container running on the harness host
- **RemoteLAN**: Service on a LAN-accessible node (no VPN required)
- **WireGuard**: Service on a remote node requiring secure tunnel

Execution types:
- **Process**: Direct process execution
- **Docker**: Container-based execution

### 2. Networking Architecture

The harness maintains a registry of all services and their IP addresses:

```
harness/
├── service-registry/
│   ├── postgres
│   │   ├── type: docker
│   │   ├── host-addr: 172.17.0.2:5432
│   │   └── wireguard-addr: 10.42.0.2:5432 (if mesh active)
│   ├── api-server
│   │   ├── type: process
│   │   ├── host-addr: 127.0.0.1:8080
│   │   └── wireguard-addr: 10.42.0.1:8080 (if mesh active)
│   ├── cache-server
│   │   ├── type: process (on LAN node)
│   │   ├── lan-addr: 192.168.1.100:6379
│   │   └── wireguard-addr: none
│   └── load-generator
│       ├── type: process (on remote node)
│       ├── public-addr: 54.23.45.67:8080
│       └── wireguard-addr: 10.42.0.10:8080 (required)
└── network-manager/
    ├── lan-interface: eth0
    ├── wireguard-interface: wg-harness (created on-demand)
    └── wireguard-subnet: 10.42.0.0/16
```

#### Network Configuration
```yaml
network:
  wireguard_subnet: 10.42.0.0/16  # Used only if remote nodes exist
  lan_interface: eth0             # For discovering LAN nodes
  dns_port: 53                    # DNS server port (5353 for non-root)
```

The system analyzes service runtime types to determine networking requirements:

- **All Local/Docker**: Direct networking via host network
- **Mix with LAN nodes**: Direct LAN networking, no WireGuard needed
- **Any Remote nodes**: Automatic WireGuard mesh creation

#### Hybrid Topology Example
```yaml
services:
  # Local process - uses host networking directly
  api-server:
    target: 
      process:
        binary: ./target/debug/api-server
        args: ["--port", "8080"]
    
  # Docker container - uses Docker networking + host bridge
  postgres:
    target:
      docker:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: test
    
  # LAN node - accessible via LAN IP, no WireGuard needed
  cache-server:
    target:
      remote_lan:
        host: 192.168.1.100
        user: deploy
        binary: /opt/cache-server/bin/cache
    
  # Remote node - triggers WireGuard mesh creation
  load-generator:
    target:
      wireguard:
        host: aws-load-test-1
        user: ubuntu
        package: ./packages/load-generator.tar.gz
```

In this example:
- Local and Docker services communicate directly
- LAN node uses existing network (192.168.1.100)
- Remote node triggers WireGuard setup
- All services connect using IP addresses provided by the harness

#### WireGuard Mesh (On-Demand)
When remote nodes are detected:

- Harness creates WireGuard interface (`wg-harness`)
- Only nodes requiring secure tunnel join the mesh
- Local/Docker/LAN services remain on direct networking
- Service discovery handles dual-stack addressing
- Automatic key generation and distribution
- Works across NAT, firewalls, different networks

#### Network Selection Logic

The network manager automatically determines the appropriate networking strategy:

1. **Analyze service targets**: Check if any services require WireGuard connectivity
2. **Conditional WireGuard setup**: Only create WireGuard interface if remote nodes exist
3. **IP assignment**:
   - Local services: Use host IP or Docker bridge IP
   - LAN services: Use their existing LAN IP
   - WireGuard services: Allocate IP from WireGuard subnet
4. **DNS registration**: All services registered with appropriate IPs for multi-homed access
5. **Peer management**: Add/remove WireGuard peers dynamically as services come and go

### 3. Service Registry and IP Resolution

The harness maintains a registry and injects appropriate IP addresses based on network topology:

**Key features:**
- Services use direct IP addresses - no DNS required
- Harness determines reachable IPs based on service locations
- Environment variables injected with correct IPs for each service
- Simple, debuggable, no DNS complexity

**IP resolution logic:**
When starting a service, the harness:
1. Determines service's network location (local, LAN, or WireGuard)
2. For each dependency, selects the appropriate IP:
   - If both on LAN: use LAN IPs
   - If either on WireGuard: use WireGuard IPs
   - If local: use host/docker bridge IPs
3. Injects environment variables with reachable addresses

**Example scenario:**
```yaml
# Service topology
postgres:    LAN (192.168.1.50)
api-server:  LAN (192.168.1.51)  
load-gen:    WireGuard (10.42.0.10, remote)

# Environment for api-server (on LAN):
POSTGRES_ADDR=192.168.1.50:5432    # Both on LAN, use LAN IP

# Environment for load-gen (on WireGuard):
API_SERVER_ADDR=10.42.0.51:8080     # Use WireGuard IP
POSTGRES_ADDR=10.42.0.50:5432       # Use WireGuard IP
```

**Service start script:**
```bash
# Harness generates env file for each service
source /tmp/harness-${RUN_ID}/service.env

# Now service can use:
psql -h ${POSTGRES_ADDR%:*} -p ${POSTGRES_ADDR#*:}
curl http://${API_SERVER_ADDR}/health
```

### 4. Service Package Format

Services are packaged as simple tarballs containing everything needed to run on a remote node:

**Package structure:**
```
service-name.tar.gz
├── manifest.yaml           # Package metadata and requirements
├── bin/
│   └── service            # Main executable (binary or script)
├── scripts/
│   ├── start.sh           # Start script (required)
│   ├── stop.sh            # Stop script (required)
│   └── health.sh          # Health check script (optional)
├── config/
│   └── config.template    # Configuration template (optional)
└── network/
    └── wireguard.conf     # WireGuard config (if needed)
```

**manifest.yaml specification:**
```yaml
name: api-server
version: 1.0.0
port: 8080                  # Primary port the service listens on

# System requirements (optional)
system_requires:
  - /usr/bin/curl          # For health checks
  - /usr/lib/libpq.so.5    # PostgreSQL client library

# Network configuration (only for WireGuard nodes)
network:
  wireguard:
    private_key: "..."     # Generated by harness
    endpoint: ${HARNESS_ENDPOINT}  # Templated value
    allowed_ips: 10.42.0.0/16
    persistent_keepalive: 25
```

**Script requirements:**

`scripts/start.sh` - Starts the service
```bash
#!/bin/bash
# Called as: ./start.sh <working_dir> <env_file>
WORK_DIR="$1"
ENV_FILE="$2"

# Load environment variables (SERVICE_ADDR values)
source "$ENV_FILE"

# Start the service
cd "$WORK_DIR"
./bin/service --port 8080 > logs/service.log 2>&1 &
echo $! > service.pid
```

`scripts/stop.sh` - Stops the service
```bash
#!/bin/bash
# Called as: ./stop.sh <working_dir>
WORK_DIR="$1"
cd "$WORK_DIR"
[ -f service.pid ] && kill $(cat service.pid)
```

`scripts/health.sh` - Checks service health (optional)
```bash
#!/bin/bash
# Exit 0 if healthy, non-zero otherwise
curl -f http://localhost:8080/health
```

**Deployment process:**
1. Harness transfers package to remote node via SSH
2. Extracts to `/opt/harness/<service-name>/`
3. Generates environment file with dependency addresses
4. Calls `start.sh` with working directory and env file
5. Monitors health using `health.sh` if provided

**Key generation and package preparation:**
- Harness can generate WireGuard keypairs for services
- Private keys embedded in service packages for autonomous connectivity
- Public keys retained by harness for peer management
- Support for pre-generated keys or harness-generated keys

**Dynamic peer management:**
- Services authenticate using their WireGuard keys
- Harness adds peers to WireGuard interface on connection
- Automatic IP allocation from configured subnet
- Peer removal on service shutdown or key revocation
- Lost keys result in immediate network access removal

**Deployment process:**
1. **Package preparation**: Generate keys, bundle binaries, create configs
2. **SSH connection**: Establish secure connection to target node
3. **Package transfer**: Upload service package to remote host
4. **Service setup**: Extract package, run setup scripts, install dependencies
5. **Network configuration**: 
   - Apply custom DNS settings if specified
   - Configure WireGuard if service has WireGuard config
   - Register service IPs with harness
6. **Service start**: Launch service with injected configuration
7. **Health monitoring**: Begin health checks and log collection

### 5. Configuration Management

Services receive configuration through environment variables and files:

```yaml
# services.yaml - Service definitions
services:
  postgres:
    target:
      docker:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: test
        ports:
          - 5432
      
  api-server:
    target:
      process:
        binary: ./target/debug/api-server
        env:
          # DATABASE_URL will be injected by harness with correct IP
      
  load-generator:
    target:
      wireguard:
        host: aws-load-test-1
        user: ubuntu
        package: ./packages/load-generator.tar.gz
    config:
      # target_url will be injected by harness with correct IP
```

## Consequences

### Positive
- **10x faster development iteration** with native process execution
- **Unified testing** across local, containerized, and distributed environments
- **Simplified debugging** with direct process access
- **Resource efficiency** by choosing appropriate runtime per service
- **Network transparency** with automatic service discovery

### Negative
- **Increased complexity** in the harness implementation
- **Platform limitations** - full features only on Linux
- **Security considerations** for SSH keys and WireGuard configuration
- **Learning curve** for new runtime options

### Mitigations
- Sensible defaults that "just work"
- Automatic feature detection and graceful degradation
- Comprehensive documentation and examples
- Security best practices built-in

## Alternatives Considered

1. **Docker Compose Extensions**: Could add custom networking but doesn't solve process execution
2. **Kubernetes**: Too heavyweight for development and testing scenarios
3. **Service Mesh (Istio/Linkerd)**: Overly complex for test environments
4. **Custom Protocol**: Unnecessary when SSH and WireGuard exist

## References

- [WireGuard](https://www.wireguard.com/) - Fast, modern VPN protocol
- [openssh-rust](https://docs.rs/openssh/) - SSH client library
- [bollard](https://docs.rs/bollard/) - Docker API client
