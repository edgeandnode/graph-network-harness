# harness-core

Core abstractions and infrastructure for building service orchestration daemons. This crate provides the foundational `Service` trait, action/event system, and daemon infrastructure used by specialized harnesses like `graph-test-daemon`.

## Architecture

### Service Trait

The heart of harness-core is the `Service` trait, which defines how services expose their functionality:

```rust
pub trait Service: Send + Sync + 'static {
    type Action: DeserializeOwned + Send;
    type Event: Serialize + Send;
    
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
}
```

Key design principles:
- **Type Safety**: Actions and events are strongly typed
- **No JSON in Domain**: JSON serialization only happens at API boundaries via `ServiceWrapper`
- **Event Streaming**: Actions return async channels for streaming events
- **Runtime Agnostic**: Works with any async runtime

### ServiceStack

`ServiceStack` manages collections of services with dynamic dispatch:

```rust
let mut stack = ServiceStack::new();

// Register services
stack.register("postgres-1".to_string(), PostgresService::new())?;
stack.register("api-1".to_string(), ApiService::new())?;

// Dispatch actions with JSON at the boundary
let events = stack.dispatch("api-1", "default", json!({
    "type": "StartServer",
    "port": 8080
})).await?;
```

### BaseDaemon

Provides core daemon functionality that specialized daemons can extend:

- WebSocket server for client connections
- Service management and registry
- Action routing and event streaming
- TLS security with certificate management

## Key Components

### Action System
- `ActionRegistry`: Register and invoke custom daemon actions
- `ActionInfo`: Metadata including parameter/return schemas
- Type-safe action handlers with JSON schema validation

### Service Management
- `ServiceManager`: Orchestrate service lifecycles
- `Registry`: Service discovery and persistence
- Network topology awareness (Local/LAN/WireGuard)

### Client Communication
- `DaemonClient`: Connect to daemons over WebSocket
- Real-time event streaming
- Request/response pattern for actions

## Usage

### Creating a Custom Service

```rust
use harness_core::{Service, Result};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
enum MyAction {
    Start { config: String },
    Stop,
    Status,
}

#[derive(Serialize)]
enum MyEvent {
    Started { pid: u32 },
    Stopped,
    Status { running: bool },
}

struct MyService {
    name: String,
}

#[async_trait]
impl Service for MyService {
    type Action = MyAction;
    type Event = MyEvent;
    
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { "My custom service" }
    
    async fn dispatch_action(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::unbounded();
        
        match action {
            MyAction::Start { config } => {
                // Start service logic
                tx.send(MyEvent::Started { pid: 12345 }).await?;
            }
            // ... handle other actions
        }
        
        Ok(rx)
    }
}
```

### Building a Daemon

```rust
use harness_core::{BaseDaemon, Daemon};

// Create daemon with services
let daemon = BaseDaemon::builder()
    .with_endpoint("127.0.0.1:9443".parse()?)
    .register_action("custom-action", "My custom action", |params| async {
        Ok(json!({ "result": "success" }))
    })?
    .build()
    .await?;

// Register services
daemon.service_stack().register("my-service", MyService::new())?;

// Start the daemon
daemon.start().await?;
```

### Connecting as a Client

```rust
use harness_core::DaemonClient;

let client = DaemonClient::connect("127.0.0.1:9443").await?;

// List services
let services = client.list_services().await?;

// Invoke service action
let events = client.invoke_service_action(
    "my-service",
    json!({ "type": "Start", "config": "production" })
).await?;

// Stream events
while let Ok(event) = events.recv().await {
    println!("Event: {:?}", event);
}
```

## Design Philosophy

1. **Separation of Concerns**: Services handle domain logic, wrappers handle JSON
2. **Type Safety**: Strongly typed actions and events with schema validation
3. **Extensibility**: Easy to add new service types and actions
4. **Runtime Agnostic**: No dependency on specific async runtime
5. **Event-Driven**: Actions produce streams of events for real-time monitoring

## Dependencies

- `async-trait`: Async trait implementations
- `async-channel`: Event streaming
- `serde`/`serde_json`: Serialization at boundaries
- `schemars`: JSON schema generation
- `async-tungstenite`: WebSocket communication
- `rustls`: TLS security

## License

Licensed under MIT or Apache-2.0, at your option.