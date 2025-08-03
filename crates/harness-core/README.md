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
- **No JSON in Domain**: JSON serialization only happens at API boundaries
- **Event Streaming**: Actions return async channels for streaming events
- **Runtime Agnostic**: Works with any async runtime

### Core Components

- **ServiceStack**: Manages collections of services with dynamic dispatch
- **BaseDaemon**: Provides WebSocket server, service management, and TLS security
- **ActionRegistry**: Register and invoke custom daemon actions with JSON schema validation
- **DaemonClient**: Connect to daemons and stream events over WebSocket

## Usage Example

```rust
use harness_core::{Service, BaseDaemon, DaemonClient};

// 1. Define a custom service
struct MyService { name: String }

#[async_trait]
impl Service for MyService {
    type Action = MyAction;  // Your action enum
    type Event = MyEvent;    // Your event enum
    
    // ... implement trait methods
}

// 2. Build a daemon
let daemon = BaseDaemon::builder()
    .with_endpoint("127.0.0.1:9443".parse()?)
    .build()
    .await?;

daemon.service_stack().register("my-service", MyService::new())?;
daemon.start().await?;

// 3. Connect as client
let client = DaemonClient::connect("127.0.0.1:9443").await?;
let events = client.invoke_service_action(
    "my-service",
    json!({ "type": "Start" })
).await?;
```

## Design Philosophy

1. **Separation of Concerns**: Services handle domain logic, wrappers handle JSON
2. **Type Safety**: Strongly typed actions and events with schema validation
3. **Extensibility**: Easy to add new service types and actions
4. **Event-Driven**: Actions produce streams of events for real-time monitoring

See `graph-test-daemon` for a complete implementation example.