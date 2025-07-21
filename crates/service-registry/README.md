# service-registry

Runtime-agnostic service registry with WebSocket API for tracking distributed services and managing deployments.

## Features

- **Runtime Agnostic**: Works with any async runtime (tokio, async-std, smol)
- **WebSocket API**: Real-time service monitoring and control
- **Service Tracking**: Register, monitor, and manage service lifecycle
- **Package Deployment**: Deploy services to remote nodes via SSH
- **Event Streaming**: Subscribe to service state changes and events

## Usage

```rust
use service_registry::{Registry, WsServer, ServiceEntry, ExecutionInfo, Location};

#[tokio::main] // or smol::main, async_std::main
async fn main() -> anyhow::Result<()> {
    // Create registry
    let registry = Registry::new();
    
    // Create WebSocket server
    let server = WsServer::new("127.0.0.1:8080", registry).await?;
    
    // Accept connections
    loop {
        let handler = server.accept().await?;
        // Choose your runtime's spawn method
        tokio::spawn(handler.handle());
    }
}
```

## WebSocket Protocol

Connect via WebSocket and send JSON messages:

```javascript
const ws = new WebSocket('ws://localhost:8080');

// List all services
ws.send(JSON.stringify({
    id: "123",
    type: "request", 
    action: "list_services",
    params: {}
}));

// Subscribe to events
ws.send(JSON.stringify({
    id: "456",
    type: "request",
    action: "subscribe", 
    params: { events: ["service_state_changed"] }
}));
```

## Architecture

The service registry follows the project's runtime-agnostic design:
- Uses `async-tungstenite` without runtime features
- File I/O via `async-fs`
- Networking via `async-net`
- Tests use `smol` runtime

## Package Format

Services are packaged as tarballs with a manifest:

```
service-name-1.0.0.tar.gz
├── manifest.yaml
├── bin/
│   └── service-executable  
├── scripts/
│   ├── start.sh
│   ├── stop.sh
│   └── health.sh
└── config/
```

Services install to sanitized paths: `/opt/{name}-{version}/`