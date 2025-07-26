# service-registry

Runtime-agnostic service discovery and network topology management with TLS-secured WebSocket API.

## Features

- **Service Discovery**: Real-time tracking of services across networks
- **Network Topology**: Automatic detection of Local, LAN, and WireGuard networks
- **TLS Security**: All connections secured with TLS (required, not optional)
- **IP Allocation**: Automatic IP address management within configured subnets
- **Event System**: Subscribe to service state changes and network events
- **Package Deployment**: Deploy service packages to remote hosts
- **Persistent Storage**: Sled database backend for reliable persistence
- **Runtime Agnostic**: Works with any async runtime

## Architecture

### Network Topology Detection

The registry automatically detects and manages three network types:

1. **Local**: Services on the same machine (127.0.0.0/8)
2. **LAN**: Services on local network (e.g., 192.168.0.0/16)
3. **WireGuard**: Services connected via WireGuard VPN

### Service Discovery

Services are automatically discoverable based on their network location:

```rust
use service_registry::{Registry, NetworkManager, ServiceEntry};

// Services register with their network location
let service = ServiceEntry {
    service_name: "api-server".to_string(),
    host_id: "prod-host-1".to_string(),
    location: Location::Lan,
    network_info: NetworkInfo {
        ip: "192.168.1.100".parse()?,
        hostname: Some("api.local".to_string()),
        // ...
    },
    // ...
};

registry.add_or_update(service).await?;

// Other services can discover it
let api = registry.get_service("api-server").await?;
let endpoint = format!("http://{}:8080", api.network_info.ip);
```

### TLS Security

TLS is always required for WebSocket connections:

```rust
use service_registry::{WsServer, TlsServerConfig};

// Server with TLS
let tls_config = TlsServerConfig::from_files(
    "certs/server.crt",
    "certs/server.key"
).await?;

let server = WsServer::new_tls("0.0.0.0:9443", registry, tls_config).await?;

// Client with TLS
let tls_config = TlsClientConfig::default()?; // Uses system root CAs
let client = WsClient::connect_tls("registry.example.com:9443", tls_config).await?;
```

## Usage

### Starting a Registry Server

```rust
use service_registry::{Registry, WsServer, TlsServerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Create registry with sled persistence
    let registry = Registry::with_persistence("./registry.db").await;
    
    // Or load existing registry
    // let registry = Registry::load("./registry.db").await?;
    
    // Load TLS certificates
    let tls_config = TlsServerConfig::from_files(
        "certs/server.crt",
        "certs/server.key"
    ).await?;
    
    // Start TLS WebSocket server
    let server = WsServer::new_tls("0.0.0.0:9443", registry, tls_config).await?;
    
    // Accept connections
    loop {
        let handler = server.accept().await?;
        tokio::spawn(handler.handle());
    }
}
```

### Client Connection

```rust
use service_registry::{WsClient, TlsClientConfig};

// Connect with TLS
let tls_config = TlsClientConfig::default()?;
let mut client = WsClient::connect_tls("localhost:9443", tls_config).await?;

// List services
let services = client.list_services().await?;

// Subscribe to events
client.subscribe(vec![
    EventType::ServiceRegistered,
    EventType::ServiceStateChanged,
    EventType::NetworkTopologyChanged,
]).await?;

// Receive events
while let Some(event) = client.next_event().await? {
    match event {
        Event::ServiceRegistered { service } => {
            println!("New service: {}", service.service_name);
        }
        // ... handle other events
    }
}
```

### Network Configuration

```yaml
# Network configuration in services.yaml
networks:
  local:
    type: local
    subnet: "127.0.0.0/8"
    
  production:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        name: "prod-api"
        ssh_user: "deploy"
        
  monitoring:
    type: wireguard
    subnet: "10.0.0.0/24"
    config_path: "/etc/wireguard/wg0.conf"
```

### IP Allocation

The registry automatically allocates IPs within configured subnets:

```rust
let network_manager = NetworkManager::new(network_config);

// Allocate IP for new service
let ip = network_manager.allocate_ip("api-server", Location::Lan)?;
// Returns: 192.168.1.2 (next available in subnet)

// Release when service stops
network_manager.release_ip("api-server")?;
```

## WebSocket API

### Request/Response Pattern

```json
// Request
{
    "id": "unique-request-id",
    "type": "request",
    "action": "list_services",
    "params": {}
}

// Response
{
    "id": "unique-request-id",
    "type": "response",
    "result": [
        {
            "service_name": "postgres",
            "host_id": "docker-host",
            "location": "local",
            "state": "running",
            "endpoints": [{"name": "db", "port": 5432}]
        }
    ]
}
```

### Available Actions

- `list_services` - Get all registered services
- `get_service` - Get specific service by name
- `list_endpoints` - Get all service endpoints
- `subscribe` - Subscribe to event types
- `unsubscribe` - Unsubscribe from events
- `deploy_package` - Deploy service package to remote host

### Event Types

- `ServiceRegistered` - New service added
- `ServiceUpdated` - Service configuration changed
- `ServiceRemoved` - Service unregistered
- `ServiceStateChanged` - Running/stopped/failed
- `NetworkTopologyChanged` - Network configuration updated
- `EndpointChanged` - Service endpoint modified

## Package Deployment

Deploy pre-built service packages to remote hosts:

```rust
use service_registry::{PackageDeployer, RemoteTarget};

let deployer = PackageDeployer::new();

let target = RemoteTarget {
    host: "192.168.1.100".to_string(),
    ssh_user: "deploy".to_string(),
    ssh_key: Some("/home/user/.ssh/deploy_key".to_string()),
    install_base: "/opt".to_string(),
};

// Deploy package
deployer.deploy_package(
    "path/to/service-1.0.0.tar.gz",
    &target,
    env_vars, // Will be injected
).await?;

// Package structure:
// service-1.0.0.tar.gz
// ├── manifest.yaml
// ├── bin/
// │   └── service
// └── scripts/
//     ├── start.sh
//     └── stop.sh
```

## Storage Backend

The registry uses [sled](https://github.com/spacejam/sled) as its embedded database:

- **Persistent Storage**: All service registrations survive restarts
- **Atomic Operations**: ACID compliance for reliability
- **Efficient Updates**: Lock-free architecture for performance
- **Crash Recovery**: Automatic recovery from unexpected shutdowns

### Database Layout

```
registry.db/
├── services/         # Service entries
└── subscriptions/    # Event subscriptions (transient, not persisted)
```

## Security Considerations

1. **TLS Required**: All WebSocket connections must use TLS
2. **Certificate Validation**: Clients validate server certificates
3. **Secure Package Transfer**: Packages deployed over SSH
4. **IP Isolation**: Services only see IPs in their network segment
5. **Persistent Storage**: Sled database files should be protected

## Testing

The crate includes comprehensive tests using Docker for network simulation:

```bash
# Run all tests including network discovery
cargo test -p service-registry --features docker-tests

# Run specific test suites
cargo test -p service-registry test_tls_connection
cargo test -p service-registry test_network_discovery
cargo test -p service-registry test_ip_allocation
```

## License

Licensed under MIT or Apache-2.0, at your option.