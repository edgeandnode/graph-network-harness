# ADR-008: WebSocket Client/Server Architecture

## Status
Accepted

## Context
The service registry requires a real-time communication mechanism for:
- Service state updates and notifications
- Event subscriptions and delivery
- Remote service control and monitoring
- Package deployment coordination

Initial implementation included placeholder WebSocket tests that were disabled via non-existent feature flags, creating a false impression of functionality.

## Decision
We implement a full WebSocket client/server architecture with the following design:

### Server Architecture
```rust
pub struct WsServer {
    registry: Arc<Registry>,
    listener: TcpListener,
}

pub struct ConnectionHandler {
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    registry: Arc<Registry>,
    subscriptions: HashSet<EventType>,
}
```

### Client Architecture
```rust
pub struct WsClient {
    ws: WebSocketStream<TcpStream>,
    addr: SocketAddr,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>>,
}

pub struct WsClientHandle {
    tx: mpsc::UnboundedSender<ClientMessage>,
    pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>>,
}
```

### Protocol Design
Request/Response pattern with event streaming:
```json
// Request
{
  "id": "uuid",
  "action": "list_services",
  "params": {}
}

// Response
{
  "id": "uuid",
  "data": {...},
  "error": null
}

// Event
{
  "event": "service_state_changed",
  "data": {...}
}
```

### Key Features
1. **Executor Agnostic**: Uses `async-tungstenite` without runtime features
2. **Concurrent Handling**: Client uses split architecture (handle + background task)
3. **Event Integration**: Server subscribes connections to Registry events
4. **Clean Shutdown**: Proper subscription cleanup on disconnect

## Implementation Details

### Server Implementation
- Accepts WebSocket connections via `async-net::TcpListener`
- Each connection maintains its own event subscriptions
- Integrates with Registry's event system for real-time updates
- Handles requests asynchronously with proper error responses

### Client Implementation
- Split architecture separates I/O from API
- Background task handles WebSocket communication
- Request/response correlation via unique IDs
- Type-safe API methods for all registry operations

### Message Flow
1. Client sends request with unique ID
2. Server processes request against Registry
3. Server sends response with matching ID
4. Events are pushed to subscribed clients asynchronously

## Consequences

### Positive
- Real WebSocket functionality instead of disabled tests
- Type-safe client API with async/await support
- Proper event subscription and delivery
- Clean separation of concerns
- Executor-agnostic implementation

### Negative
- More complex than REST API
- Requires background task for client
- Connection state management overhead

### Trade-offs
- Complexity vs real-time capabilities
- Memory usage for connection state vs immediate updates
- Background task requirement vs synchronous API

## Example Usage

### Server
```rust
let registry = Registry::new();
let server = WsServer::new("127.0.0.1:8080", registry).await?;

loop {
    let handler = server.accept().await?;
    smol::spawn(handler.handle()).detach();
}
```

### Client
```rust
let client = WsClient::connect(addr).await?;
let (handle, handler) = client.start_handler().await;

// Run handler in background
smol::spawn(handler).detach();

// Use the handle
let services = handle.list_services().await?;
handle.subscribe(vec![EventType::ServiceStateChanged]).await?;
```

## Future Enhancements
1. WebSocket compression support
2. Reconnection logic for clients
3. Rate limiting and backpressure
4. Authentication and authorization
5. Binary message support for package transfers