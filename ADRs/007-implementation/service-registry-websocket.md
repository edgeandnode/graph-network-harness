# Service Registry WebSocket Design

## Overview
The service registry uses a WebSocket-only API for all interactions, providing real-time updates and control over distributed services.

## Architecture

### Runtime Agnosticism
Following the project's CODE-POLICY, the service registry is designed as a runtime-agnostic library:
- Uses `async-tungstenite` without runtime-specific features
- Networking via `async-net` (works with any runtime)
- File I/O via `async-fs` 
- Tests use `smol` runtime
- Binary consumers choose their runtime

### Core Components

```
┌─────────────────────┐     ┌──────────────────┐
│   WebSocket Client  │────▶│  WebSocket Server│
└─────────────────────┘     └──────────────────┘
                                     │
                            ┌────────▼─────────┐
                            │     Registry     │
                            │  ┌────────────┐  │
                            │  │ServiceStore│  │
                            │  └────────────┘  │
                            │  ┌────────────┐  │
                            │  │Event System│  │
                            │  └────────────┘  │
                            └──────────────────┘
                                     │
                            ┌────────▼─────────┐
                            │ File Persistence │
                            └──────────────────┘
```

## WebSocket Protocol

### Message Format

All messages use JSON with a type discriminator:

```typescript
// Client → Server
interface Request {
  id: string;          // UUID for correlation
  type: "request";
  action: Action;
  params: object;
}

// Server → Client
interface Response {
  id: string;          // Matches request ID
  type: "response";
  data?: any;
  error?: ErrorInfo;
}

// Server → Client (pushed)
interface Event {
  type: "event";
  event: EventType;
  data: object;
}
```

### Actions

```typescript
type Action = 
  | "list_services"      // Get all services
  | "get_service"        // Get specific service
  | "service_action"     // Start/stop/restart
  | "list_endpoints"     // Get all endpoints
  | "deploy_package"     // Deploy to remote
  | "subscribe"          // Subscribe to events
  | "unsubscribe";       // Stop receiving events
```

### Event Types

```typescript
type EventType =
  | "service_state_changed"  // Service started/stopped
  | "endpoint_updated"       // Address changed
  | "deployment_progress"    // Deploy status
  | "health_check_result"    // Health status
  | "registry_loaded";       // Initial state
```

## Data Models

### ServiceEntry

```rust
pub struct ServiceEntry {
    /// Unique service identifier
    pub name: String,
    
    /// Service version
    pub version: String,
    
    /// Execution context
    pub execution: ExecutionInfo,
    
    /// Where it runs
    pub location: Location,
    
    /// Network endpoints
    pub endpoints: Vec<Endpoint>,
    
    /// Dependencies
    pub depends_on: Vec<String>,
    
    /// Current state
    pub state: ServiceState,
    
    /// Last health check
    pub last_health_check: Option<HealthStatus>,
}

pub enum ExecutionInfo {
    ManagedProcess {
        pid: Option<u32>,
        command: String,
    },
    DockerContainer {
        container_id: String,
        image: String,
    },
    SystemdService {
        unit_name: String,
    },
}

pub enum Location {
    Local,
    Remote {
        host: String,
        ssh_config: SshConfig,
    },
}

pub struct Endpoint {
    pub name: String,           // e.g., "http", "grpc"
    pub address: SocketAddr,
    pub protocol: Protocol,
}

pub enum ServiceState {
    Registered,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed { reason: String },
}
```

## Package Format

### Structure
```
service-name-1.2.3.tar.gz
├── manifest.yaml
├── bin/
│   └── service-executable
├── scripts/
│   ├── start.sh      # Required
│   ├── stop.sh       # Required
│   └── health.sh     # Optional
└── config/
    └── default.conf
```

### Manifest Specification
```yaml
name: api-server
version: 1.2.3
description: REST API server

# Service metadata
service:
  type: managed_process  # or docker, systemd
  ports:
    - name: http
      port: 8080
      protocol: tcp

# Dependencies
depends_on:
  - postgres
  - redis

# System requirements
requires:
  commands:
    - /usr/bin/curl
  libraries:
    - libpq.so.5

# Health check
health:
  script: scripts/health.sh
  interval: 30s
  timeout: 5s
```

### Deployment Path
Services are installed to sanitized paths:
- Pattern: `/opt/{sanitized-name}-{version}/`
- Example: `/opt/api-server-1.2.3/`
- Sanitization: `[^a-zA-Z0-9_-]` → `_`

## Runtime-Agnostic Implementation

### WebSocket Server

```rust
use async_net::TcpListener;
use async_tungstenite::{accept_async, WebSocketStream};
use futures::{StreamExt, SinkExt};

pub struct WsServer {
    registry: Arc<Registry>,
}

impl WsServer {
    pub async fn listen(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        
        // Return connection handlers - caller spawns
        loop {
            let (stream, addr) = listener.accept().await?;
            let registry = self.registry.clone();
            
            // Yield handler - no spawning!
            yield ConnectionHandler::new(stream, addr, registry);
        }
    }
}
```

### Connection Handler

```rust
pub struct ConnectionHandler {
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    registry: Arc<Registry>,
    subscriptions: HashSet<EventType>,
}

impl ConnectionHandler {
    // Pure async - caller decides execution
    pub async fn handle(mut self) -> Result<()> {
        while let Some(msg) = self.ws.next().await {
            match msg? {
                Message::Text(text) => {
                    let request: Request = serde_json::from_str(&text)?;
                    let response = self.process_request(request).await;
                    let json = serde_json::to_string(&response)?;
                    self.ws.send(Message::Text(json)).await?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Ok(())
    }
}
```

## Event System

### Subscription Management

```rust
impl Registry {
    // Returns events - no spawning
    pub fn emit_event(&self, event: Event) -> Vec<(SocketAddr, Event)> {
        let subscribers = self.subscribers.lock().unwrap();
        subscribers
            .iter()
            .filter(|(_, sub)| sub.is_subscribed(&event.event_type))
            .map(|(addr, _)| (*addr, event.clone()))
            .collect()
    }
}
```

### Event Delivery Pattern

The registry generates events but doesn't deliver them. The server layer handles delivery:

```rust
// In the binary/server layer
let events = registry.emit_event(event);
for (addr, event) in events {
    if let Some(conn) = connections.get(&addr) {
        // Binary chooses how to send (spawn, select, etc)
        runtime_specific_send(conn, event).await;
    }
}
```

## Security Considerations

### Package Deployment
- Validate package signatures (future)
- Sanitize all paths
- Check manifest before execution
- Run scripts with limited privileges

### WebSocket
- Basic authentication via token
- Connection limits
- Message size limits
- Rate limiting

## Testing Strategy

### Unit Tests
```rust
#[smol_potat::test]
async fn test_service_registration() {
    let registry = Registry::new();
    let service = ServiceEntry::new("api", "1.0.0");
    registry.register(service).await.unwrap();
    assert_eq!(registry.list().await.len(), 1);
}
```

### Integration Tests
```rust
#[smol_potat::test]
async fn test_websocket_flow() {
    let registry = Registry::new();
    let server = WsServer::new(registry);
    
    // Test with mock TCP streams
    let (client, server) = async_net::TcpStream::pair()?;
    
    // Full protocol test
}
```

## Example Usage

### JavaScript Client
```javascript
class RegistryClient {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.pending = new Map();
    
    this.ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      if (msg.type === 'response') {
        const handler = this.pending.get(msg.id);
        handler?.(msg);
      } else if (msg.type === 'event') {
        this.emit(msg.event, msg.data);
      }
    };
  }
  
  async request(action, params) {
    const id = crypto.randomUUID();
    const promise = new Promise(resolve => {
      this.pending.set(id, resolve);
    });
    
    this.ws.send(JSON.stringify({
      id, type: 'request', action, params
    }));
    
    return promise;
  }
}

// Usage
const client = new RegistryClient('ws://localhost:8080');
const services = await client.request('list_services', {});
```

### Rust Binary
```rust
// Choose your runtime in the binary
#[tokio::main]  // or async-std, or smol
async fn main() {
    let registry = Registry::load_or_create("registry.json").await?;
    let server = WsServer::new(registry);
    
    server.serve("127.0.0.1:8080", |handler| {
        tokio::spawn(handler.handle());
    }).await?;
}
```
