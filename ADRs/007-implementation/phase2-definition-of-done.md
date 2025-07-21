# Phase 2: Core Infrastructure - Definition of Done

**Status: In Progress**

Last Updated: 2025-01-21

## Objective
Create a runtime-agnostic service registry with WebSocket API for tracking distributed services, and a package management system for deploying services to remote nodes.

## Deliverables

### 1. Service Registry Core ✓ When:
- [ ] Data models implemented (ServiceEntry, Endpoint, Location, ExecutionInfo)
- [ ] In-memory store with async Mutex for thread safety
- [ ] CRUD operations for service management
- [ ] Service state tracking (Running, Stopped, Failed)
- [ ] Endpoint resolution by service name
- [ ] Dependency tracking between services
- [ ] Event generation for state changes
- [ ] No runtime-specific code (tokio, async-std)

### 2. File Persistence ✓ When:
- [ ] JSON serialization of registry state
- [ ] Atomic file writes using async-fs
- [ ] Load registry from disk on startup
- [ ] Auto-save on state changes (debounced)
- [ ] Handle corrupted files gracefully
- [ ] Backup previous state before overwrite
- [ ] Tests for save/load cycle

### 3. WebSocket Server ✓ When:
- [ ] Uses async-tungstenite without runtime features
- [ ] Accepts connections via async-net TcpListener
- [ ] Message protocol implementation (request/response/event)
- [ ] Connection handler that's runtime-agnostic
- [ ] Event subscription system
- [ ] Broadcast events to subscribers
- [ ] Connection lifecycle management
- [ ] Error handling and reconnection support

### 4. WebSocket Protocol ✓ When:
- [ ] JSON message format defined
- [ ] Request/Response correlation via ID
- [ ] Actions implemented:
  - [ ] list_services
  - [ ] get_service
  - [ ] service_action (start/stop/restart)
  - [ ] list_endpoints
  - [ ] deploy_package
  - [ ] subscribe/unsubscribe
- [ ] Event types defined:
  - [ ] service_state_changed
  - [ ] endpoint_updated
  - [ ] deployment_progress
  - [ ] health_check_result
- [ ] Protocol versioning for future compatibility

### 5. Package Management ✓ When:
- [ ] Package format specification (manifest.yaml, scripts, binaries)
- [ ] Package name/version sanitization
- [ ] Install path: `/opt/{name}-{version}/`
- [ ] Package builder from local directory
- [ ] Manifest validation
- [ ] Tarball creation with proper permissions
- [ ] Package extraction and verification
- [ ] Atomic deployment (extract to temp, then move)
- [ ] Rollback support (keep previous version)

### 6. Deployment Integration ✓ When:
- [ ] Integration with command-executor for remote deployment
- [ ] SSH transfer of package files
- [ ] Remote extraction and setup
- [ ] Environment variable generation for services
- [ ] Start script execution with proper context
- [ ] Deployment progress reporting via WebSocket
- [ ] Error handling and cleanup on failure

### 7. Testing ✓ When:

#### Unit Tests
- [ ] Registry CRUD operations
- [ ] Message protocol parsing
- [ ] Package name sanitization
- [ ] Event filtering and delivery
- [ ] Persistence layer
- [ ] Connection management

#### Integration Tests  
- [ ] Full WebSocket connection cycle
- [ ] Multi-client event delivery
- [ ] Package deployment flow
- [ ] Service lifecycle management
- [ ] Concurrent access patterns
- [ ] Error recovery scenarios

#### Runtime Agnostic Tests
- [ ] All tests use smol runtime
- [ ] No runtime-specific imports in library code
- [ ] Verify works with different runtimes in examples

### 8. Documentation ✓ When:
- [ ] Module-level documentation
- [ ] WebSocket protocol specification
- [ ] Package format specification
- [ ] API usage examples
- [ ] Architecture diagrams
- [ ] No missing_docs warnings

### 9. Binary Implementation ✓ When:
- [ ] `harness-ws-server` binary created
- [ ] Can choose runtime (example with smol)
- [ ] Command-line arguments for configuration
- [ ] Graceful shutdown handling
- [ ] Logging integration

### 10. API Quality ✓ When:
- [ ] Ergonomic API design
- [ ] Type-safe message handling
- [ ] Clear error types with context
- [ ] Consistent naming conventions
- [ ] Future-proof protocol design
- [ ] Follows Rust best practices

## Success Criteria

Phase 2 is complete when:
1. All checkboxes above are checked
2. Service registry tracks distributed services without imposing runtime choice
3. WebSocket API enables real-time monitoring and control
4. Package deployment works reliably to remote nodes
5. Test coverage > 80% for core functionality
6. Can be used as a library with any async runtime
7. Example implementations show usage with different runtimes

## Example Usage

**Library Usage:**
```rust
use service_registry::{Registry, WsServer};
use smol; // or tokio, or async-std

// Runtime-agnostic library
let registry = Registry::new();
let server = WsServer::new("127.0.0.1:8080", registry).await?;

// User chooses how to run it
loop {
    let handler = server.accept().await?;
    smol::spawn(handler.handle()).detach(); // or tokio::spawn
}
```

**WebSocket Client:**
```javascript
const ws = new WebSocket('ws://localhost:8080');
ws.send(JSON.stringify({
    id: "123",
    type: "request",
    action: "list_services",
    params: {}
}));
```

## Non-Goals (Future Phases)
- Distributed consensus
- Service discovery beyond local registry
- Authentication/authorization (basic only)
- Metrics collection
- Web UI dashboard