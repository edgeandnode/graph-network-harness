# ADR-009: Network Testing Strategy with Docker Compose

## Status
Accepted

## Context
Phase 3 of the graph-network-harness implementation requires sophisticated network topology detection and routing logic. Testing these features requires:
- Multiple network configurations (local, LAN, WireGuard)
- Service discovery across network boundaries
- IP address allocation and resolution
- WireGuard configuration generation

Traditional testing approaches would require:
- Physical hardware for LAN testing
- Root access for WireGuard interface creation
- Complex network setup that's hard to reproduce
- Manual verification of network connectivity

## Decision
We will use Docker Compose to simulate various network topologies for comprehensive integration testing. This approach allows us to:
1. Create isolated networks that simulate real network conditions
2. Test service discovery and IP resolution logic
3. Validate WireGuard configuration without requiring root access
4. Ensure reproducible test environments across development machines and CI

## Implementation Details

### Test Infrastructure Structure
```
crates/service-registry/tests/
├── network_tests/
│   ├── docker-compose/
│   │   ├── local-only.yml         # All services on same host
│   │   ├── lan-simple.yml         # Multiple services on LAN
│   │   ├── lan-multiple.yml       # Complex LAN with subnets
│   │   ├── mixed-topology.yml     # Local + LAN + Remote simulation
│   │   └── network-partition.yml  # Failure scenarios
│   ├── network_discovery_tests.rs
│   ├── ip_resolution_tests.rs
│   └── wireguard_config_tests.rs
```

### Test Scenarios

#### 1. Local-Only Network
Tests services running on the same host (processes and Docker containers) using bridge networks.

#### 2. LAN Simulation
Uses Docker networks with specific subnets to simulate LAN environments where services can directly reach each other via IP.

#### 3. Multiple Networks
Tests scenarios where the harness must route between different network segments, simulating complex enterprise networks.

#### 4. Mixed Topology
Combines local, LAN, and isolated networks to test WireGuard requirement detection and configuration generation.

### Example Test Implementation
```rust
#[test]
fn test_mixed_network_topology() {
    smol::block_on(async {
        // Start docker-compose environment
        setup_docker_compose("mixed-topology.yml").await;
        
        // Create network manager
        let nm = NetworkManager::new(test_config());
        
        // Discover network topology
        let topology = nm.discover_topology().await.unwrap();
        
        // Verify correct network detection
        assert_eq!(topology.get("local-service").unwrap().location, NetworkLocation::Local);
        assert_eq!(topology.get("lan-service").unwrap().location, NetworkLocation::RemoteLAN);
        assert_eq!(topology.get("remote-service").unwrap().location, NetworkLocation::WireGuard);
        
        // Test IP resolution logic
        let ip = nm.resolve_service_ip("local-service", "remote-service").await.unwrap();
        assert!(ip.to_string().starts_with("10.42.")); // WireGuard subnet
        
        cleanup_docker_compose("mixed-topology.yml").await;
    });
}
```

### Docker Compose Configuration Example
```yaml
version: '3.8'
services:
  # Simulated harness host
  harness:
    image: alpine:latest
    networks:
      lan:
        ipv4_address: 192.168.100.10
      local:
        ipv4_address: 172.30.0.10
        
  # LAN service
  lan-service:
    image: alpine:latest
    networks:
      lan:
        ipv4_address: 192.168.100.20
        
  # Isolated service (requires WireGuard)
  remote-service:
    image: alpine:latest
    networks:
      remote:
        ipv4_address: 10.0.0.20

networks:
  lan:
    driver: bridge
    ipam:
      config:
        - subnet: 192.168.100.0/24
  local:
    driver: bridge
    ipam:
      config:
        - subnet: 172.30.0.0/16
  remote:
    driver: bridge
    internal: true  # Isolated network
    ipam:
      config:
        - subnet: 10.0.0.0/24
```

## Consequences

### Positive
- **No Root Required**: Tests run entirely in user space
- **Reproducible**: Identical network topology every test run
- **Fast**: Containers start in seconds, enabling rapid iteration
- **CI-Friendly**: Works seamlessly in GitHub Actions and other CI systems
- **Realistic**: Accurately simulates network isolation and routing
- **Debuggable**: Can exec into containers to verify connectivity
- **Comprehensive**: Can test edge cases and failure scenarios easily

### Negative
- **Docker Dependency**: Requires Docker to be installed and running
- **Resource Usage**: Multiple containers consume more resources than unit tests
- **Complexity**: Test setup is more complex than simple unit tests
- **Maintenance**: Docker compose files need updates as requirements change

### Neutral
- Tests mock WireGuard configuration generation but don't test actual WireGuard connectivity
- Network performance characteristics differ from real networks
- Some platform-specific network behaviors may not be captured

## Alternatives Considered

1. **Unit Tests Only**: Would miss integration issues and real network behavior
2. **Physical Test Lab**: Too expensive and hard to reproduce
3. **Virtual Machines**: Slower and more resource-intensive than containers
4. **Network Namespaces**: Requires root access, platform-specific
5. **Mocking Everything**: Would not catch real networking issues

## References
- Docker Compose Networking: https://docs.docker.com/compose/networking/
- Container Network Model: https://github.com/containernetworking/cni
- ADR-007: Implementation plan specifying test-driven development