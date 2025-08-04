# Runtime-Agnostic Migration Plan

This document outlines the migration plan for converting all remaining runtime-specific spawning patterns to use the runtime-agnostic `async-runtime-compat` crate.

## Current Status

✅ **COMPLETED**:
- Created `async-runtime-compat` crate with `Spawner` trait
- Migrated `TaskWrapper` and `ServiceWrapper` in harness-core
- Updated all harness-core integration tests to use `SmolSpawner`
- Fixed trait object compatibility issues

## Remaining Migration Work

### High Priority - Production Code

#### 1. WebSocket Server (harness/src/daemon/server.rs)
**Lines**: 100, 101-117

```rust
// Current implementation
smol::spawn(async move {
    // Accept TLS connection
    match tls_acceptor.accept(stream).await {
        // ... handler logic
    }
}).detach();
```

**Migration needed**: The WebSocket server spawns handlers for each connection. This needs to use the Spawner trait.

**Impact**: High - this is production server code
**Files**: `crates/harness/src/daemon/server.rs`

#### 2. Graph Test Daemon Task (graph-test-daemon/src/tasks.rs)
**Lines**: 153

```rust
// Current implementation  
smol::spawn(async move {
    while let Some(event) = event_stream.next().await {
        if let Some(translated) = process_hardhat_event(&event, &mut deployed_contracts, &mut completed) {
            let _ = tx.send(translated).await;
        }
    }
}).detach();
```

**Migration needed**: Event processing background task needs spawner injection.

**Impact**: Medium - domain-specific daemon functionality
**Files**: `crates/graph-test-daemon/src/tasks.rs`

#### 3. Service Registry WebSocket Handlers (service-registry/src/lib.rs)
**Pattern**: Server loops that spawn connection handlers

**Migration needed**: The service registry WebSocket server likely uses spawning patterns similar to the harness daemon.

**Impact**: High - core service discovery functionality
**Files**: Need to investigate `crates/service-registry/src/lib.rs`

### Medium Priority - Test Code

#### 4. Command Executor Integration Tests
**Files**: 
- `crates/command-executor/tests/stdin_forwarding_integration.rs` (lines 64, 120, 167)
- `crates/command-executor/tests/stdin_layered_tests.rs`
- `crates/command-executor/tests/launcher_tests.rs`

```rust
// Current pattern
smol::spawn(async move {
    let _ = stdin_handle.forward_channel().await;
}).detach();
```

**Migration needed**: Convert test helpers to accept spawner parameter.

**Impact**: Low - test code only, but should be consistent

#### 5. Service Registry Tests
**Files**:
- `crates/service-registry/tests/websocket_tests.rs` (multiple locations)
- `crates/service-registry/tests/tls_tests.rs` (multiple locations)

```rust
// Current pattern
smol::spawn(handler.handle()).detach();
```

**Migration needed**: Test WebSocket servers need spawner injection.

**Impact**: Low - test code

#### 6. Test Infrastructure Cleanup
**File**: `crates/command-executor/tests/common/test_harness.rs` (line 297)

```rust
// Current implementation
std::thread::spawn(move || {
    smol::block_on(async move {
        let _ = harness.teardown().await;
    });
});
```

**Migration needed**: This is problematic - using `std::thread::spawn` with `smol::block_on`. Should be redesigned.

**Impact**: Medium - affects test reliability

### Low Priority - Library Internal Use

#### 7. Service Orchestration Process Executor
**Files**: `crates/service-orchestration/src/executors/process.rs`

**Investigation needed**: Check if this has spawning patterns that need migration.

#### 8. Documentation Examples
**Files**: 
- `crates/command-executor/README.md` (line 127)
- Documentation and ADR examples

**Migration needed**: Update code examples to show spawner injection pattern.

## Migration Strategy

### Phase 1: Core Infrastructure (Week 1)

1. **Update WebSocket Server Pattern**
   - Modify `harness/src/daemon/server.rs` to accept spawner injection
   - Update server creation to pass spawner through
   - Test with all runtime backends

2. **Service Registry Investigation**
   - Audit `service-registry/src/lib.rs` for spawning patterns
   - Create migration plan for WebSocket server components
   - Ensure consistency with harness daemon patterns

### Phase 2: Domain Logic (Week 2)

3. **Graph Test Daemon Migration**
   - Update `GraphTestDaemon` to accept spawner in constructor
   - Modify task execution to use injected spawner
   - Update any tests that create the daemon

4. **Service Orchestration Audit**
   - Review process executor for spawning patterns
   - Migrate if necessary to maintain consistency

### Phase 3: Test Infrastructure (Week 3)

5. **Test Helper Migration**
   - Update command-executor test helpers to accept spawner
   - Convert service-registry test servers to use spawner injection
   - Fix test harness cleanup to avoid `std::thread::spawn`

6. **Documentation Updates**
   - Update all code examples in READMEs
   - Update ADR examples
   - Add spawner injection patterns to coding guidelines

## Implementation Guidelines

### API Design Patterns

1. **Constructor Injection** (Preferred for production code):
```rust
pub struct WebSocketServer {
    spawner: Arc<dyn Spawner>,
    // ... other fields
}

impl WebSocketServer {
    pub fn new(spawner: Arc<dyn Spawner>) -> Self {
        Self { spawner }
    }
    
    async fn handle_connection(&self, stream: TcpStream) {
        let spawner = self.spawner.clone();
        let future = Box::pin(async move {
            // Connection handling logic
        });
        spawner.spawn(future);
    }
}
```

2. **Parameter Injection** (For flexible APIs):
```rust
pub async fn start_event_processing<S: Spawner>(
    events: impl Stream<Item = Event>,
    spawner: &S,
) -> Result<()> {
    let future = Box::pin(async move {
        // Event processing logic
    });
    spawner.spawn(future);
    Ok(())
}
```

3. **Test Utilities** (For test code):
```rust
#[cfg(test)]
pub fn create_test_server() -> WebSocketServer {
    WebSocketServer::new(Arc::new(SmolSpawner))
}
```

### Testing Strategy

1. **Multi-Backend Testing**:
   - Each migrated component should be tested with at least Smol and Tokio spawners
   - Use feature gates to conditionally test different backends

2. **Integration Tests**:
   - Ensure spawner injection doesn't break existing functionality
   - Test that different spawner implementations are compatible

3. **Performance Validation**:
   - Verify that spawner abstraction doesn't introduce significant overhead
   - Compare performance before/after migration

## Dependencies and Blockers

### Required Crate Updates

1. **Add async-runtime-compat dependency** to:
   - `harness` crate (for daemon server)
   - `graph-test-daemon` crate (for task processing)
   - `service-registry` crate (if spawning patterns found)

2. **Feature flag strategy**:
   - Enable appropriate runtime features in dev-dependencies
   - Consider optional features for different runtime backends

### API Compatibility

1. **Breaking Changes**:
   - WebSocket server constructor will require spawner parameter
   - Graph test daemon constructor will require spawner parameter
   - Consider providing backward-compatible constructors with default spawners

2. **Default Runtime Selection**:
   - Decide on default spawner for production use
   - Document runtime selection strategy for users

## Success Criteria

### Functional Requirements
- [ ] All production code uses `Spawner` trait instead of direct runtime calls
- [ ] No `std::thread::spawn` usage in library code
- [ ] All tests pass with both Smol and Tokio spawners
- [ ] Documentation examples use spawner injection pattern

### Performance Requirements
- [ ] No more than 5% performance regression in spawning operations
- [ ] Memory usage remains within acceptable bounds
- [ ] Integration test execution time doesn't increase significantly

### Code Quality Requirements
- [ ] All spawning follows consistent patterns across codebase
- [ ] Error handling for spawner operations is robust
- [ ] Code is well-documented with spawner usage examples

## Timeline

- **Week 1**: Phase 1 - Core infrastructure migration
- **Week 2**: Phase 2 - Domain logic migration  
- **Week 3**: Phase 3 - Test infrastructure and documentation
- **Week 4**: Testing, validation, and cleanup

## Notes

1. **Backward Compatibility**: Consider providing wrapper functions with default spawners to maintain API compatibility during transition.

2. **Runtime Detection**: For cases where spawner type can't be injected, consider runtime detection of available async runtime.

3. **Error Handling**: Spawner operations should have consistent error handling patterns across the codebase.

4. **Testing Philosophy**: Maintain the current testing approach using smol, but ensure code works with other runtimes when spawners are injected.