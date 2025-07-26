# ISSUES-BACKLOG.md

This file tracks issues discovered during code audits. Issues are organized by priority.

## Critical Issues

### 1. Unsafe unwrap after empty check
- **Location**: harness/src/daemon/server.rs:83
- **Problem**: Logic error - code checks if `keys` is empty then uses `.unwrap()` anyway
- **Current code**: 
  ```rust
  if keys.is_empty() {
      return Err(anyhow::anyhow!("No private keys found in key file"));
  }
  let key = PrivateKey(keys.into_iter().next().unwrap());
  ```
- **Fix**: Replace with `.ok_or_else()` pattern:
  ```rust
  let key = PrivateKey(keys.into_iter().next()
      .ok_or_else(|| anyhow::anyhow!("No private keys found in key file"))?);
  ```

## High Priority Issues

### 1. CODE-POLICY violation: Re-exports in lib.rs files
- **Locations**: 
  - command-executor/src/lib.rs (8+ pub use statements)
  - service-registry/src/lib.rs (9+ pub use statements)  
  - harness-core/src/lib.rs (extensive prelude re-exports)
  - service-orchestration/src/lib.rs (5+ pub use statements)
  - graph-test-daemon/src/lib.rs (3+ pub use statements)
- **Problem**: CODE-POLICY explicitly states "No Re-exports: Never use `pub use` in lib.rs"
- **Fix**: Remove all re-exports; users must import types from their defining modules (e.g., `use command_executor::error::Error`)

### 2. Missing error_set! macro implementation
- **Problem**: CODE-POLICY mandates using `error_set!` macro for composable error types, but it's not used anywhere in the codebase
- **Impact**: Error handling doesn't follow the documented composition pattern
- **Fix**: Implement error composition using the error_set! macro as specified in CODE-POLICY

### 3. Extensive incomplete implementation (41 TODOs)
- **Major gaps**:
  - harness-core/src/client.rs: 6 TODOs - stub implementation
  - service-orchestration/src/package.rs: 10 TODOs - package deployment missing
  - graph-test-daemon/src/actions.rs: 8 TODOs - action implementations incomplete
  - service-orchestration/src/executors/remote.rs: 5 TODOs - remote execution not implemented
  - service-registry/src/websocket.rs: Package deployment handler missing
- **Fix**: Create GitHub issues to track and prioritize implementation of missing features

## Medium Priority Issues

### 1. Runtime-specific dependencies in service-orchestration
- **Location**: service-orchestration/Cargo.toml
- **Problem**: Optional tokio/async-std dependencies violate runtime-agnostic principle
- **Current**: 
  ```toml
  tokio = { version = "1.0", features = ["full"], optional = true }
  async-std = { version = "1.12", optional = true }
  ```
- **Fix**: Remove runtime-specific dependencies or clearly document why they're necessary

### 2. Potential panics in production code
- **Locations**:
  - graph-test-daemon/src/lib.rs:27: `.parse().unwrap()` on hardcoded address
  - graph-test-daemon/src/lib.rs:56: `panic!("Expected Process target")`
  - harness-core/src/service.rs: Similar panic pattern
- **Fix**: Replace panics with proper error handling using Result types

### 3. Inconsistent error handling patterns
- **Problem**: No use of error composition despite error_set! being documented
- **Impact**: Error types are not composable as intended by architecture
- **Fix**: Refactor error types to use the error_set! pattern consistently

## Low Priority Issues

### 1. Missing deny.toml for runtime-agnostic crates
- **Problem**: Only command-executor has deny.toml to ban runtime-specific deps
- **Fix**: Add deny.toml to service-registry, harness-core, and other runtime-agnostic crates

### 2. Documentation lacks examples
- **Problem**: CODE-POLICY requires examples in documentation, but most docs are minimal
- **Fix**: Add usage examples to public API documentation

### 3. Dependency version inconsistency
- **Problem**: Mix of workspace dependencies and direct version specifications
- **Fix**: Move all common dependencies to workspace level for consistency

## Completed Issues

---
*Last audit run: 2025-07-25*