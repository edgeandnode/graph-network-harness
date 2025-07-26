# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Building
```bash
# Build all crates
cargo build --workspace

# Build release version
cargo build --release --workspace

# Build the CLI binary
cargo build --bin harness
```

### Testing
```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p command-executor
cargo test -p harness
cargo test -p service-registry

# Run integration tests
cargo test --test '*' --workspace

# Run tests with output
cargo test --workspace -- --nocapture

# Run tests with feature flags
cargo test -p service-registry --features docker-tests
cargo test -p service-registry --features integration-tests
```

### Running Tests
```bash
# Run service-registry network discovery tests
cargo test -p service-registry --features docker-tests

# Run command-executor tests 
cargo test -p command-executor --features ssh-tests,docker-tests

# Run all workspace tests
cargo test --workspace
```

### Linting and Validation
```bash
# Check dependency constraints
cargo deny check

# Format code
cargo fmt --all

# Run clippy
cargo clippy --workspace --all-targets

# Check documentation
cargo doc --workspace --no-deps
```

## High-Level Architecture

### Project Structure
The project is a Rust workspace implementing a heterogeneous service orchestration system:

- **command-executor**: Runtime-agnostic command execution across backends (local, SSH, Docker)
- **service-registry**: Service discovery, network topology detection, and WebSocket-based management
- **Future: orchestrator**: New harness implementing ADR-007 heterogeneous service orchestration

### Key Architectural Principles

1. **Runtime Agnosticism**: Library crates MUST work with any async runtime (Tokio, async-std, smol). Use async-process, async-fs, async-net instead of runtime-specific alternatives.

2. **Docker-in-Docker (DinD)**: The harness uses DinD to provide complete isolation. Each test run gets its own Docker daemon, preventing conflicts from hardcoded container names in local-network.

3. **Session-Based Logging**: All test runs create persistent log sessions in `test-activity/logs/` with timestamps and unique IDs. Logs survive test completion for debugging.

4. **Error Composition**: Uses `thiserror` and the `error_set!` macro for composable error types. Errors should be domain-specific and use `#[from]` for conversions.

5. **No Re-exports**: Never use `pub use` in lib.rs. Users must import types from their defining modules (e.g., `use command_executor::error::Error`).

### Command Execution Architecture
The command-executor crate provides a unified interface for running commands across:
- Local processes (via async-process)
- SSH connections (via openssh)
- Docker containers (planned)
- Systemd services (in development)

Each backend implements the `CommandExecutor` trait with runtime-agnostic async methods.

### Service Management
The harness manages services through:
- Docker Compose for containerized services (graph-node, postgres, IPFS, etc.)
- Process launchers for local execution
- SSH attachers for remote services
- Future: Systemd integration for production-like environments

### Testing Infrastructure
- Uses smol for runtime-agnostic test execution
- Integration tests use actual Docker containers with pre-configured services
- Self-hosted tests validate the harness itself using Docker-in-Docker

## Important Patterns and Conventions

### Async Code
```rust
// Always use runtime-agnostic libraries
use async_process::Command;  // Good
// use tokio::process::Command;  // Bad - tied to tokio

// Use async-trait for public APIs
#[async_trait]
pub trait Executor {
    async fn execute(&self, cmd: Command) -> Result<Output>;
}
```

### Error Handling
```rust
// Use thiserror with error_set! macro
use thiserror::Error;
use error_set::error_set;

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("Failed to spawn: {0}")]
    SpawnFailed(String),
}

error_set! {
    CommandError = ProcessError || NetworkError;
}
```

### Dependencies
- Check workspace dependencies first in root Cargo.toml
- Use `dep = { workspace = true }` when available
- Run `cargo deny check` before commits
- Runtime-agnostic crates must ban tokio/async-std in deny.toml

### Documentation
- All public APIs must have doc comments
- Include examples in documentation
- Document panics, errors, and safety requirements
- Use `#![warn(missing_docs)]` in library crates

## Critical Files and Locations

- `CODE-POLICY.md`: Detailed coding standards and architectural decisions
- `ADRs/`: Architecture Decision Records documenting major design choices
- `test-activity/`: Generated test artifacts and logs (gitignored)
- `docker-test-env/`: Docker-in-Docker setup for isolated testing
- Root `Cargo.toml`: Workspace configuration with shared dependencies
- `deny.toml`: Dependency constraints and license requirements

## Testing Considerations

### Core Testing Philosophy
**TEST EARLY AND OFTEN**: Phase completion is defined by passing tests, not just code implementation.

- Write tests BEFORE or ALONGSIDE implementation, not after
- Each feature should have unit tests AND integration tests
- Use TDD (Test-Driven Development) when practical
- A phase is NOT complete until all tests pass reliably

### Testing Practices
- Always use smol for async tests to maintain runtime agnosticism
- Integration tests require Docker to be running
- Use docker-compose to simulate complex network topologies
- Test various scenarios: local, LAN, WireGuard, mixed networks
- Use `--keep-running` flag for interactive debugging
- Check `test-activity/logs/current/` for real-time log output
- Session logs persist after test completion for post-mortem analysis

### Test Execution Verification
**CRITICAL**: Always check the actual test count in cargo test output to verify tests are running:
- Look for "running X tests" in the output - if it's "running 0 tests", tests are being filtered out
- Common causes: missing feature flags, incorrect test names, cfg conditions
- Example: `cargo test network_discovery_tests` shows "running 0 tests" → need `--features docker-tests`
- Always verify the expected number of tests actually run, don't just check for "test result: ok"

### Integration Testing Strategy
- **Local Tests**: Single machine scenarios (processes, Docker containers)
- **LAN Tests**: Use docker-compose networks to simulate LAN topologies
- **WireGuard Tests**: Mock WireGuard behavior without requiring root
- **End-to-End Tests**: Full system tests with real services
```

## Development Practices

### Documentation and Planning
- Don't add time estimates in docs or plans - this is not going to be accurate

## Workflow and Communication

### Task Execution and Communication
- You will be given instructions to complete a task. If you find yourself reversing course completely on the task ALWAYS stop and explain yourself and ask first. If I say we are going to do X, and you say "no I'm getting too many compilation errors" let's not do X, then STOP AND ASK FIRST

### Reversing Course Guidelines
- When encountering difficulties (compilation errors, API issues, design challenges), you should:
  1. First attempt to solve the problem
  2. If stuck, explain the specific issue and ask for guidance
  3. NEVER abandon the current approach for an easier one without explicit approval
  4. NEVER rationalize changing course with phrases like "for now", "to simplify", "for MVP" without asking

### Examples of Reversing Course Requiring Permission
- Switching from TLS to plain connections because of compile errors
- Changing from a daemon architecture to something simpler
- Removing features that were explicitly requested

## Rust Type Safety and Mutability

### Mutability Guidelines
- Whenever we use Mutex, Arc, or RefCell in the codebase, we should ALWAYS note in the struct doc comments what we're intending with internal or external mutability
- When making notes about internal mutability, refer to that not what primitive we are using for it.

## Development Practices

- ALWAYS use cargo to build, don't revert to using rustc directly for testing.

## Workflow Guidance

### Commit Message Generation
- When asked to generate a commit message, run git diff --staged and generate one from that

## Guidelines for Code Auditing
- When asked to audit the code for issues, put the details into ISSUES-BACKLOG.md

## Async Testing Practices

- When writing async unit tests, use smol_potat::test annotation