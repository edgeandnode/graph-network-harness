# ADR-010: Bare LAN Node Support

## Status

Proposed

## Context

Not all test scenarios require WireGuard mesh networking. Many development and testing environments have:
- Multiple machines on the same LAN (home/office network)
- Direct connectivity without NAT traversal needs
- No requirement for encrypted traffic (trusted network)
- Desire for simpler setup without WireGuard installation

Currently, the design assumes all remote nodes will join the WireGuard mesh, which adds complexity for simple LAN testing scenarios.

## Decision

Support "bare" LAN nodes as a fourth runtime type that:
1. Connects via SSH for deployment and management
2. Uses actual LAN IP addresses (e.g., 192.168.x.x)
3. Doesn't require WireGuard installation
4. Integrates with the same service discovery system

## Consequences

### Positive Consequences

- **Simpler setup**: No WireGuard required for LAN testing
- **Better performance**: Direct LAN communication without tunnel overhead
- **Easier adoption**: Lower barrier to entry for basic distributed testing
- **Resource efficiency**: No CPU overhead from encryption

### Negative Consequences

- **No encryption**: Traffic between nodes is unencrypted
- **LAN-only**: Doesn't work across networks without VPN
- **IP management**: Must handle actual LAN IPs vs mesh IPs

### Risks

- **Security**: Unencrypted traffic on LAN
- **Network conflicts**: Potential IP/port conflicts with existing services
- **Discovery complexity**: Services need to handle both mesh and LAN addresses

## Implementation

### Service Definition
```yaml
services:
  gpu-indexer:
    runtime: bare_lan
    host: 192.168.1.100
    ssh_key: ~/.ssh/id_rsa
    network:
      type: bare
      advertise_ip: 192.168.1.100
      ports:
        - name: http
          port: 7600
```

### Runtime Type
```rust
enum ServiceRuntime {
    Process,
    Container, 
    Remote { host: String, ssh_key: PathBuf },     // Uses WireGuard
    BareLAN { host: String, ssh_key: PathBuf },    // No WireGuard
}
```

### Service Discovery
- Bare LAN nodes register their actual LAN IP in the service registry
- DNS server returns appropriate IP based on caller location
- Services can discover each other using DNS names

### Implementation Steps

- [ ] Add BareLAN runtime type
- [ ] Implement bare node deployment without WireGuard setup
- [ ] Update service registry to handle LAN addresses
- [ ] Modify DNS resolver to return best route (LAN preferred over mesh)
- [ ] Add network reachability detection
- [ ] Update documentation for bare node setup

## Alternatives Considered

### Alternative 1: Require WireGuard everywhere
- Description: Keep current design, require mesh for all remote nodes
- Pros: Consistent security model, single network type
- Cons: Overkill for LAN testing, higher setup complexity
- Why rejected: Adds unnecessary complexity for common use cases

### Alternative 2: SSH tunnels only
- Description: Use SSH port forwarding instead of service types
- Pros: Works everywhere SSH works
- Cons: Complex management, poor performance, limited scalability
- Why rejected: Doesn't scale well with many services

### Alternative 3: VPN for everything
- Description: Require existing VPN setup (OpenVPN, etc.)
- Pros: Reuse existing infrastructure
- Cons: External dependency, complex configuration
- Why rejected: Want self-contained solution