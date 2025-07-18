# ADR-011: DNS-Based Service Discovery

## Status

Proposed

## Context

Services in the harness need to discover and connect to each other across different runtime environments:
- Local processes (localhost)
- Docker containers (bridge network)
- Remote nodes (WireGuard mesh IPs)
- Bare LAN nodes (actual LAN IPs)

Currently, service discovery would require:
- Hardcoded addresses
- Environment variable injection
- Manual /etc/hosts management
- Complex configuration templates

We need a unified, standard way for services to discover each other regardless of where they run.

## Decision

Implement a DNS server within the harness that:
1. Serves as the authoritative source for the `.service.local` domain
2. Dynamically updates from the service registry
3. Returns appropriate IPs based on network topology
4. Runs on both mesh (10.42.0.1:53) and LAN (0.0.0.0:5353) interfaces

Services use standard DNS names like `postgres.service.local` and the harness DNS server resolves to the best available endpoint.

## Consequences

### Positive Consequences

- **Zero configuration**: Services use DNS names without knowing network topology
- **Standard protocol**: Works with any application that does DNS lookups
- **Dynamic updates**: Service changes reflected immediately
- **Intelligent routing**: Returns best IP based on caller location
- **Debugging friendly**: Can use standard DNS tools (dig, nslookup)

### Negative Consequences

- **Complexity**: Running a DNS server adds complexity
- **Port conflicts**: May conflict with existing DNS servers
- **Caching issues**: DNS TTLs need careful management

### Risks

- **DNS hijacking**: Must ensure DNS server only responds for .service.local
- **Performance**: DNS lookup overhead (minimal with caching)
- **Reliability**: DNS server becomes critical infrastructure

## Implementation

### DNS Server Architecture
```rust
use trust_dns_server::{ServerFuture, Authority};

struct HarnessDnsServer {
    registry: Arc<ServiceRegistry>,
    zone: String,  // "service.local"
}

impl Authority for HarnessDnsHandler {
    async fn lookup(&self, name: &Name, rtype: RecordType) -> Result<DnsResponse> {
        // postgres.service.local -> postgres
        let service_name = parse_service_name(name)?;
        
        // Look up in registry
        let endpoints = self.registry.resolve(&service_name).await?;
        
        // Return best endpoint for caller
        Ok(build_response(endpoints))
    }
}
```

### Service Configuration
```yaml
# Services use DNS names
services:
  graph-node:
    env:
      POSTGRES_URL: postgres://postgres.service.local:5432
      IPFS_URL: http://ipfs.service.local:5001
```

### DNS Resolution Logic
```rust
impl ServiceRegistry {
    fn resolve_for_caller(&self, service: &str, caller_ip: IpAddr) -> Option<IpAddr> {
        let record = self.get(service)?;
        
        // Priority order:
        // 1. Same host (localhost)
        // 2. Same network (LAN)
        // 3. Mesh network
        // 4. Any available
        
        record.endpoints
            .iter()
            .find(|e| self.is_optimal_route(e, caller_ip))
            .map(|e| e.address)
    }
}
```

### Implementation Steps

- [ ] Integrate trust-dns-server
- [ ] Create RegistryDnsHandler that queries service registry
- [ ] Implement intelligent endpoint selection
- [ ] Add DNS server to harness startup
- [ ] Configure services to use DNS names
- [ ] Add DNS troubleshooting commands
- [ ] Document DNS setup for different platforms

## Alternatives Considered

### Alternative 1: Environment variables only
- Description: Inject all service addresses as env vars
- Pros: Simple, no DNS server needed
- Cons: Static, requires restart for changes, complex with many services
- Why rejected: Not dynamic enough for development

### Alternative 2: Custom service discovery protocol
- Description: Implement custom HTTP/gRPC discovery API
- Pros: More control, rich metadata
- Cons: Requires client libraries, non-standard
- Why rejected: DNS is standard and requires no client changes

### Alternative 3: Consul/etcd
- Description: Use existing service discovery tool
- Pros: Feature-rich, production-ready
- Cons: External dependency, complex setup
- Why rejected: Want self-contained solution

### Alternative 4: mDNS/Avahi
- Description: Use multicast DNS for LAN discovery
- Pros: Zero configuration, standard protocol
- Cons: LAN-only, doesn't work with mesh
- Why rejected: Doesn't support our mesh network needs