# Service Registry Design

## Overview
The service registry tracks all services, their locations, execution types, and network endpoints.

## Core Data Model

```rust
pub struct ServiceEntry {
    /// Unique service identifier
    pub name: String,
    
    /// How the service is executed
    pub execution: ExecutionInfo,
    
    /// Where the service runs
    pub location: Location,
    
    /// Network endpoints
    pub endpoints: Vec<Endpoint>,
    
    /// Service dependencies
    pub requires: Vec<String>,
    
    /// Current state
    pub state: ServiceState,
    
    /// Health check info
    pub health: HealthConfig,
}

pub enum ExecutionInfo {
    ManagedProcess {
        pid: Option<u32>,
        binary: PathBuf,
    },
    SystemdService {
        unit_name: String,
    },
    SystemdPortable {
        image_name: String,
        unit_name: String,
    },
    DockerContainer {
        container_id: String,
        image: String,
    },
    ComposeService {
        compose_file: PathBuf,
        service_name: String,
        stack_id: String,  // Links to ComposeStack
    },
}

pub enum Location {
    Local,
    Remote {
        host: String,
        ssh_user: String,
    },
}

pub struct Endpoint {
    /// What kind of address this is
    pub endpoint_type: EndpointType,
    
    /// The actual address
    pub address: SocketAddr,
    
    /// Which networks can reach this endpoint
    pub reachable_from: Vec<NetworkScope>,
}

pub enum EndpointType {
    /// Direct host networking
    Host,
    
    /// Docker internal network
    DockerInternal { network: String },
    
    /// Docker published port
    DockerPublished,
    
    /// WireGuard mesh address
    WireGuard,
    
    /// LAN address
    Lan,
}

pub enum NetworkScope {
    /// Same host only
    Localhost,
    
    /// Same docker network
    DockerNetwork(String),
    
    /// Same LAN
    Lan,
    
    /// WireGuard mesh
    WireGuard,
    
    /// Public internet
    Public,
}
```

## Registry Operations

```rust
impl ServiceRegistry {
    /// Register a new service
    pub async fn register(&mut self, entry: ServiceEntry) -> Result<()>;
    
    /// Get endpoint for service based on caller's location
    pub async fn resolve_endpoint(
        &self,
        service: &str,
        from_location: &Location,
        from_network: &NetworkScope,
    ) -> Result<SocketAddr>;
    
    /// Get all services in a compose stack
    pub async fn get_stack_services(&self, stack_id: &str) -> Vec<&ServiceEntry>;
    
    /// Update service state
    pub async fn update_state(&mut self, service: &str, state: ServiceState) -> Result<()>;
}
```

## Example Registry State

```yaml
services:
  # Postgres in docker-compose
  postgres:
    execution_info:
      compose_service:
        compose_file: ./docker-compose.yml
        service_name: postgres
        stack_id: local-stack
    location: local
    endpoints:
      - endpoint_type: docker_internal
        network: harness_default
        address: 172.22.0.2:5432
        reachable_from: [docker_network_harness_default]
      - endpoint_type: docker_published
        address: 127.0.0.1:5432
        reachable_from: [localhost]
      - endpoint_type: host
        address: 192.168.1.50:5432
        reachable_from: [lan, wireguard]
    
  # API server as managed process
  api-server:
    execution_info:
      managed_process:
        pid: 12345
        binary: ./bin/api-server
    location: local
    endpoints:
      - endpoint_type: host
        address: 127.0.0.1:8080
        reachable_from: [localhost]
      - endpoint_type: lan
        address: 192.168.1.50:8080
        reachable_from: [lan]
      - endpoint_type: wireguard
        address: 10.42.0.1:8080
        reachable_from: [wireguard]
    requires: [postgres, redis]
    
  # Remote systemd service
  monitoring:
    execution_info:
      systemd_service:
        unit_name: prometheus
    location:
      remote:
        host: 192.168.1.100
        ssh_user: ops
    endpoints:
      - endpoint_type: lan
        address: 192.168.1.100:9090
        reachable_from: [lan]
      - endpoint_type: wireguard
        address: 10.42.0.100:9090
        reachable_from: [wireguard]
```

## Endpoint Resolution Logic

When resolving endpoints, the registry:

1. Finds all endpoints for the requested service
2. Filters by what's reachable from the caller's network scope
3. Prioritizes by network efficiency:
   - Same network (localhost, docker network) 
   - Direct connection (LAN)
   - Tunneled connection (WireGuard)
4. Returns the best available endpoint

This handles the complexity of mixed execution types transparently!