# ADR-005: Service Event Streaming and Inspection API

## Status

Accepted

## Context

While the local-network-harness provides excellent capabilities for running and managing the Graph Protocol stack in isolated environments, users need better visibility into what's happening with individual services during test execution. Current logging captures raw output, but doesn't provide structured insights into service behavior, events, or state changes.

Key requirements identified:
1. **Real-time Service Inspection**: Ability to monitor individual services as they run
2. **Event-driven Analysis**: Parse service logs into structured events (startup, errors, health checks, etc.)
3. **Pluggable Architecture**: Support service-specific event parsing without requiring all services to be implemented upfront
4. **Library API**: Programmatic access for tools and automation, not just CLI
5. **Incremental Adoption**: Ability to start with generic event parsing and add service-specific handlers over time

The Graph Protocol stack includes many microservices (graph-node, indexer-agent, postgres, chain, ipfs, etc.), each with different log formats and event patterns. Maintaining parsing logic for all services would be a significant maintenance burden if done in a monolithic way.

## Decision

We will implement a trait-based, pluggable service event streaming API that allows incremental implementation of service-specific event handlers while providing a generic fallback for services without custom parsing.

### Core Architecture

#### Event System (`src/inspection/events.rs`)
```rust
#[derive(Debug, Clone, Serialize)]
pub struct ServiceEvent {
    pub timestamp: DateTime<Utc>,
    pub service_name: String,
    pub event_type: EventType,
    pub severity: EventSeverity,
    pub message: String,
    pub data: serde_json::Value, // Flexible service-specific data
}

#[derive(Debug, Clone, Serialize)]
pub enum EventType {
    Started,
    Stopped,
    Error,
    HealthCheck,
    StateChange,
    Custom(String),
}
```

#### Async Service Event Handler Trait (`src/inspection/handlers.rs`)
```rust
#[async_trait]
pub trait ServiceEventHandler: Send + Sync {
    async fn handle_log_line(&self, line: &str) -> Option<ServiceEvent>;
    async fn handle_container_event(&self, event: &ContainerEvent) -> Option<ServiceEvent>;
    fn service_name(&self) -> &str;
}
```

#### Service Event Registry (`src/inspection/registry.rs`)
```rust
pub struct ServiceEventRegistry {
    handlers: HashMap<String, Box<dyn ServiceEventHandler>>,
    fallback_handler: GenericEventHandler,
}

impl ServiceEventRegistry {
    pub fn register_handler(&mut self, handler: Box<dyn ServiceEventHandler>);
    pub async fn process_log_line(&self, service: &str, line: &str) -> Option<ServiceEvent>;
}
```

#### Async Streaming API (`src/inspection/streamer.rs`)
```rust
pub struct ServiceInspector {
    registry: ServiceEventRegistry,
    log_streamer: LogStreamer,
}

impl ServiceInspector {
    pub fn event_stream(&self) -> impl Stream<Item = ServiceEvent> + '_;
    pub fn service_stream(&self, service_name: &str) -> impl Stream<Item = ServiceEvent> + '_;
}
```

### Built-in Handlers
- **GenericEventHandler**: Fallback for services without custom parsing
- **PostgresEventHandler**: Database startup, connection errors, query logs
- **GraphNodeEventHandler**: Indexing progress, subgraph deployment, sync status
- **IndexerAgentEventHandler**: Allocation events, cost model updates, decision making

### Integration Points
- Extend `LocalNetworkHarness` with `start_service_inspection()` method
- Build on existing `LogSession` and `LogStreamer` infrastructure
- Provide async Stream interface for real-time event consumption

## Consequences

### Positive Consequences

- **Incremental Implementation**: Start with generic events, add specific parsers over time
- **Non-breaking**: Builds on existing logging infrastructure without breaking changes
- **Pluggable Architecture**: Users can register custom handlers for their specific services
- **Type Safety**: Rust traits ensure proper handler implementation with compile-time checks
- **Async Streaming**: Real-time event consumption with backpressure handling
- **Maintenance Distribution**: Service teams can implement their own event handlers

### Negative Consequences

- **Initial Complexity**: More complex than simple log parsing
- **Learning Curve**: Users need to understand trait implementation for custom handlers
- **Performance Overhead**: Additional parsing layer on top of existing logging

### Risks

- **Handler Quality**: Poorly implemented handlers could affect overall system performance
- **Event Schema Evolution**: Changes to ServiceEvent structure could break handlers
- **Resource Usage**: Multiple event streams could consume significant memory/CPU

## Implementation

### Phase 1: Core Infrastructure ✅
- [x] Add async-trait dependency to Cargo.toml
- [x] Create `/src/inspection/` module structure
- [x] Implement core event types and ServiceEvent struct
- [x] Build ServiceEventHandler trait and registry system

### Phase 2: Basic Handlers ✅
- [x] Implement GenericEventHandler for fallback parsing
- [x] Create PostgresEventHandler for database events
- [x] Build GraphNodeEventHandler for indexing events
- [ ] Add IndexerAgentEventHandler for agent-specific events (future)

### Phase 3: Streaming API ✅
- [x] Implement ServiceInspector with async Stream interface
- [x] Integrate with existing LogStreamer infrastructure
- [x] Add service-specific event filtering and routing
- [x] Build backpressure handling for high-volume event streams

### Phase 4: Integration ✅
- [x] Extend LocalNetworkHarness with inspection capabilities
- [ ] Add CLI commands for real-time event monitoring (future)
- [x] Create example usage patterns and documentation
- [x] Add comprehensive tests for event parsing and streaming

## Alternatives Considered

### Alternative 1: Monolithic Event Parsing
- **Description**: Implement all service event parsing in a single module
- **Pros**: Simpler architecture, centralized parsing logic, easier to maintain consistency
- **Cons**: High maintenance burden, difficult to add new services, tight coupling
- **Why rejected**: Doesn't scale with the number of microservices in the Graph Protocol stack

### Alternative 2: Configuration-Based Parsing
- **Description**: Use YAML/JSON configuration files to define log parsing rules
- **Pros**: No code changes needed for new services, runtime configuration
- **Cons**: Limited parsing complexity, difficult debugging, no type safety
- **Why rejected**: Too limited for complex service-specific parsing needs

### Alternative 3: External Event Processing
- **Description**: Send all logs to external system (e.g., Fluentd, Logstash) for parsing
- **Pros**: Industry standard, powerful processing capabilities, scalable
- **Cons**: External dependencies, complex setup, overkill for local testing
- **Why rejected**: Too much infrastructure overhead for local development workflow

### Alternative 4: Pre-defined Event Types Only
- **Description**: Define a fixed set of event types that all services must conform to
- **Pros**: Consistent event schema, easier analysis, simpler implementation
- **Cons**: Inflexible, doesn't capture service-specific events, limits usefulness
- **Why rejected**: Too restrictive for the diverse needs of different microservices