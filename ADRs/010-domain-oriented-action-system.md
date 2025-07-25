# ADR-010: Domain-Oriented Action System

## Status
Proposed

## Context

The current harness provides generic service orchestration capabilities, but integration testing of complex distributed systems requires:

1. **Test-specific service configurations** that go beyond production deployments
2. **Programmable test scenarios** with precise control over timing and state
3. **Domain-aware assertions** that understand the system being tested
4. **Reproducible test environments** with deterministic behavior
5. **Rich test actions** like fault injection, state verification, and performance analysis

Currently, integration testing with the harness requires:
- Writing external test scripts that shell out to the CLI
- Losing type safety between test code and service orchestration
- Difficulty in synchronizing test assertions with service state
- Limited ability to inject test-specific behavior
- No direct access to service internals for verification

## Decision

We will implement a library-first architecture that allows users to extend the base daemon with domain-specific capabilities:

### 1. Core Library Architecture

**Note**: The `harness-core` library will be created to expose existing functionality as a unified API:

```rust
// harness-core will wrap and re-export functionality from:
// - service-orchestration (ServiceManager, executors)
// - service-registry (Registry, network management)
// - harness-config (YAML parsing, validation)
// - New abstractions for extensibility

harness-core/
├── daemon/          # Daemon trait and base implementation
├── service/         # ServiceType trait and derive macro
├── action/          # Action trait and discovery protocol
├── client/          # Client libraries for daemon communication
├── builder/         # Fluent builder APIs
└── testing/         # Test utilities and helpers
```

### 2. Domain-Specific Extensions

Users create custom daemons by extending the base:

```rust
use harness_core::{Daemon, ServiceType, Action, Result};

// Define domain-specific service types
#[derive(Debug, Clone, ServiceType)]
#[service_type(name = "graph-node")]
pub struct GraphNode {
    pub ethereum_rpc: String,
    pub ipfs_endpoint: String,
    pub postgres_url: String,
    
    #[service_type(runtime_field)]
    pub admin_endpoint: String,
}

// Extend daemon with domain knowledge
pub struct GraphDaemon {
    base: harness_core::BaseDaemon,
    graph_state: GraphState,
}

impl Daemon for GraphDaemon {
    fn new() -> Result<Self> {
        let base = BaseDaemon::builder()
            .with_service_manager()      // From service-orchestration
            .with_service_registry()     // From service-registry
            .with_config_parser()        // From harness-config
            .register_service_type::<GraphNode>()
            .register_action("deploy-subgraph", Self::deploy_subgraph)
            .register_action("check-sync", Self::check_sync_status)
            .build()?;
            
        Ok(Self {
            base,
            graph_state: GraphState::new(),
        })
    }
    
    async fn deploy_subgraph(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        // Implementation using existing ServiceManager
        let service_manager = self.base.service_manager();
        // ... deployment logic ...
    }
}
```

### 3. Action Discovery Protocol

The daemon exposes available actions through its API:

```json
// Request: List available actions
{"type": "list_actions"}

// Response: Action definitions with schemas
{
  "type": "actions",
  "actions": [
    {
      "name": "deploy-subgraph",
      "description": "Deploy a subgraph to the network",
      "params_schema": { /* JSON Schema */ },
      "returns_schema": { /* JSON Schema */ }
    }
  ]
}

// Request: Invoke action
{
  "type": "invoke_action",
  "action": "deploy-subgraph",
  "params": {
    "manifest_url": "https://...",
    "deployment_name": "uniswap-v3"
  }
}
```

### 4. CLI Integration

The standard CLI discovers and invokes actions without needing domain knowledge:

```bash
# CLI connects to daemon and discovers available actions
harness action list

# CLI can show action details with parameter info
harness action info deploy-subgraph

# CLI parses arguments and invokes action
harness action run deploy-subgraph --manifest-url https://... --deployment-name uniswap-v3
```

## Consequences

### Positive

1. **Type Safety**: Domain logic uses Rust's type system fully
2. **Performance**: No serialization overhead, direct function calls
3. **Flexibility**: Users can create highly specialized daemons
4. **Discoverability**: CLI can work with any daemon implementing the protocol
5. **Testing**: Domain logic can be unit tested without full system
6. **Distribution**: Users distribute their own daemon binaries
7. **Reusability**: Leverages existing harness crates

### Negative

1. **Compilation Required**: Changes require rebuilding the daemon
2. **Version Coordination**: Daemon and action code must be compatible
3. **Distribution Complexity**: Users must distribute binaries, not just scripts
4. **Learning Curve**: Requires Rust knowledge for extensions
5. **Initial Complexity**: Need to create harness-core abstraction layer

### Neutral

1. **No Runtime Plugins**: Intentional design choice for safety and performance
2. **Binary Size**: Custom daemons include all functionality
3. **Process Model**: Each domain daemon is a separate process

## Example: Integration Test Implementation

See [ADR-011: Graph Test Daemon](./011-graph-test-daemon.md) for a complete example of implementing this pattern for The Graph Protocol integration testing.

```rust
// Example from graph-test-daemon
use harness_core::prelude::*;

pub struct GraphTestDaemon {
    base: BaseDaemon,
    test_state: Arc<Mutex<TestState>>,
}

impl GraphTestDaemon {
    pub fn new() -> Result<Self> {
        let base = BaseDaemon::builder()
            // Reuse existing service types
            .with_standard_services()
            // Add Graph-specific services
            .register_service_type::<AnvilBlockchain>()
            .register_service_type::<GraphNode>()
            // Register test actions
            .register_action("deploy-subgraph", Self::deploy_subgraph)
            .register_action("inject-reorg", Self::inject_blockchain_reorg)
            .build()?;
            
        Ok(Self { base, test_state: Default::default() })
    }
}
```

## Implementation Plan

### Phase 1: Core Library Creation (Prerequisites)
- Create `harness-core` crate structure
- Define `Daemon`, `ServiceType`, and `Action` traits
- Wrap existing service-orchestration functionality
- Create builder pattern for daemon construction

### Phase 2: Service Type System  
- Implement service type registration
- Create derive macro for `#[derive(ServiceType)]`
- Add service lifecycle hooks
- Integrate with existing ServiceManager

### Phase 3: Action System
- Define action trait and registration
- Implement discovery protocol
- Add JSON schema generation
- Create CLI action commands

### Phase 4: Example Implementation
- Build GraphTestDaemon as reference (see ADR-011)
- Document patterns and best practices
- Create templates for common domains

## Alternatives Considered

### 1. Runtime Plugin System (Rejected)
- Would allow dynamic loading of plugins
- Rejected due to: ABI compatibility issues, security concerns, performance overhead

### 2. Subprocess Actions Only (Rejected)
- Would use stdin/stdout for all extensions
- Rejected due to: Serialization overhead, limited type safety, complexity for rich operations

### 3. Configuration-Only Extensions (Rejected)
- Would use YAML/JSON for all customization
- Rejected due to: Lack of type safety, limited expressiveness, string-based programming

## References

- [ADR-007: Distributed Service Orchestration](./007-distributed-service-orchestration.md)
- [ADR-011: Graph Test Daemon](./011-graph-test-daemon.md) - First implementation of this pattern
- [Extend vs Embed Principle](https://www.teamten.com/lawrence/programming/extend-vs-embed.html)
- [Kubernetes Operator Pattern](https://kubernetes.io/docs/concepts/extend-kubernetes/operator/)

## Decision Outcome

This design provides a powerful extension mechanism that maintains type safety and performance while enabling rich domain-specific orchestration. The library-first approach allows us to leverage existing harness functionality while providing clean abstractions for extensibility.

The GraphTestDaemon (ADR-011) serves as the proof of concept for this architecture.