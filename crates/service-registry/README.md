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

## Architecture

The registry provides three core functions:

1. **Service Registration**: Services register with their network location (Local/LAN/WireGuard)
2. **Discovery**: Other services query for endpoints based on network topology
3. **Events**: Real-time notifications of service state changes

## WebSocket API

All communication happens over TLS-secured WebSocket connections using a request/response pattern:

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
    "result": [...]
}
```

### Available Actions
- `list_services` - Get all registered services
- `get_service` - Get specific service by name
- `list_endpoints` - Get all service endpoints
- `subscribe` - Subscribe to event types
- `deploy_package` - Deploy service package to remote host

### Event Types
- `ServiceRegistered` - New service added
- `ServiceUpdated` - Service configuration changed
- `ServiceStateChanged` - Running/stopped/failed
- `NetworkTopologyChanged` - Network configuration updated

## Quick Start

```rust
use service_registry::{Registry, WsServer, TlsServerConfig};

// Start registry server
let registry = Registry::with_persistence("./registry.db").await;
let tls_config = TlsServerConfig::from_files("server.crt", "server.key").await?;
let server = WsServer::new_tls("0.0.0.0:9443", registry, tls_config).await?;

// Client connection
use service_registry::{WsClient, TlsClientConfig};
let client = WsClient::connect_tls("localhost:9443", TlsClientConfig::default()?).await?;
let services = client.list_services().await?;
```

## Package Deployment

Deploy pre-built service packages to remote hosts via SSH. Packages follow a standard format with manifest, binaries, and lifecycle scripts.

## Testing

```bash
cargo test -p service-registry --features docker-tests
```