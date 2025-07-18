# ADR-007: WireGuard Mesh Networking for Distributed Services

## Status

Proposed

## Context

The local-network-harness needs to orchestrate services across different execution environments:
- Native processes on the harness host
- Docker containers (local or remote daemons)
- Remote processes on other Linux machines (accessed via SSH)

Currently, services running in Docker use Docker's bridge networking, which doesn't extend to:
- Services running as native processes
- Services on remote machines
- Cross-host container communication

We need a unified networking solution that allows services to discover and communicate with each other regardless of where they run.

## Decision

Implement a WireGuard-based mesh network using the `wireguard-control` Rust crate, creating a flat network topology where all services can communicate directly.

Key design decisions:
1. Use a single WireGuard interface (`wg-harness`) managed by the orchestrator
2. Allocate IPs from 10.42.0.0/16 subnet
3. Each service gets a unique mesh IP regardless of runtime type
4. No coordination server - the harness manages all peer relationships
5. Platform-specific support: Full mesh on Linux, localhost fallback on other platforms

## Consequences

### Positive Consequences

- **Unified addressing**: Services use consistent `service-name.mesh.local` addresses
- **Location transparency**: Services don't need to know where other services run
- **Security**: All mesh traffic encrypted by WireGuard
- **Performance**: Direct peer-to-peer connections without proxies
- **Flexibility**: Easy to add new nodes/services dynamically

### Negative Consequences

- **Platform limitations**: Full functionality requires Linux
- **Complexity**: More complex than simple port forwarding
- **Network overhead**: WireGuard adds some overhead vs direct connections

### Risks

- **Key management**: Need secure distribution of WireGuard keys
- **NAT traversal**: May require STUN/keepalives for remote nodes
- **Debugging complexity**: Encrypted traffic harder to debug

## Implementation

### Service Configuration
```yaml
services:
  graph-node:
    network:
      type: mesh  # Options: mesh, host, dual
      ports: [8000, 8020]
```

### Architecture
```
┌─────────────────────┐
│   Harness Host      │
│  ┌───────────────┐  │     ┌──────────────┐
│  │ Service Mesh  │  │     │ Remote Host  │
│  │  10.42.0.1    │  │────▶│  10.42.0.10  │
│  └───────────────┘  │     └──────────────┘
│         │           │
│  ┌──────┴────────┐  │     ┌──────────────┐
│  │ Local Process │  │     │   Container  │
│  │  10.42.0.2    │  │────▶│  10.42.0.11  │
│  └───────────────┘  │     └──────────────┘
└─────────────────────┘
```

### Implementation Steps

- [ ] Create ServiceMesh struct using wireguard-control
- [ ] Implement IP allocation from subnet pool
- [ ] Add WireGuard peer configuration generation
- [ ] Create remote node enrollment via SSH
- [ ] Implement service discovery with mesh IPs
- [ ] Add platform detection and fallback logic
- [ ] Create health checking for mesh connectivity

## Alternatives Considered

### Alternative 1: Innernet
- Description: CIDR-based WireGuard mesh with coordination server
- Pros: Mature, feature-rich, good access control
- Cons: Requires coordination server, not available as library
- Why rejected: Too complex for our use case, adds external dependency

### Alternative 2: SSH Tunnels
- Description: Create SSH tunnels between all nodes
- Pros: No additional software, works everywhere SSH works
- Cons: O(n²) connections, complex management, performance overhead
- Why rejected: Doesn't scale, difficult to manage dynamic services

### Alternative 3: Tailscale/Headscale
- Description: Managed WireGuard mesh network
- Pros: Easy setup, good NAT traversal
- Cons: External service dependency, not self-contained
- Why rejected: Want self-contained solution without external dependencies

### Alternative 4: Container-only networking
- Description: Limit to Docker networks, no remote/native support
- Pros: Simple, well-understood
- Cons: Can't support native processes or remote services
- Why rejected: Doesn't meet requirements for hybrid execution
