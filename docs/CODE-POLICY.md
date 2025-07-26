# Code Policy

This document outlines the coding standards and architectural decisions for the graph-network-harness project.

## Async Runtime Agnosticism

Library crates in this project MUST be designed to work with any async runtime.

### Core Principles:
- **No runtime assumptions**: Code should work whether the user is using Tokio, async-std, smol, or any other runtime
- **Runtime-agnostic dependencies**: Prefer crates from the smol ecosystem (async-process, async-fs, async-net) which work with any runtime
- **Let the user choose**: Binary crates may pick a runtime, but libraries must remain neutral

### Guidelines:

#### Use Runtime-Agnostic Crates:
```rust
// Good - works with any runtime
use async_process::Command;
use async_fs::File;
use async_net::TcpStream;

// Bad - tied to specific runtimes
use tokio::process::Command;
use async_std::fs::File;
```

#### Avoid Runtime-Specific Features:
```rust
// Bad - requires tokio
tokio::spawn(async { ... });
tokio::time::sleep(duration).await;
tokio::select! { ... }

// Good - runtime agnostic
// Let the caller handle spawning
// Use futures::future::select instead of tokio::select!
```

#### Use async-trait for Public APIs:
```rust
use async_trait::async_trait;

#[async_trait]
pub trait Executor {
    async fn execute(&self, cmd: Command) -> Result<Output>;
}
```

#### Handle Spawning via Traits:
If you need to spawn tasks, accept a spawner:
```rust
pub fn new<S>(spawner: S) -> Self 
where 
    S: futures::task::Spawn + Send + 'static
{
    // ...
}
```

### Testing with Different Runtimes:
- Use smol for tests - it's minimal and runtime-agnostic
- Consider testing with multiple runtimes in CI
- Document which runtime features are required for tests

### cargo-deny Configuration:
Runtime-agnostic crates should include:
```toml
[bans]
deny = [
    { name = "tokio" },
    { name = "tokio-*" },
    { name = "async-std" },
]
```

## Error Handling

Use `thiserror` for error enums in library crates, with modular error composition.

### Guidelines:
- Define error types using `thiserror::Error` derive macro
- Use the `error_set!` macro for composable error types
- Keep error types focused and domain-specific
- Use `#[from]` for automatic conversions
- Prefer specific error types over generic ones

### Example with error_set!:
```rust
use thiserror::Error;
use error_set::error_set;

#[derive(Error, Debug)]
pub enum ProcessError {
    #[error("Failed to spawn process: {0}")]
    SpawnFailed(String),
    
    #[error("Process terminated with signal: {0}")]
    SignalTerminated(i32),
}

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

error_set! {
    CommandError = ProcessError || NetworkError;
}
```

## Dependency Management

Use `cargo-deny` to enforce dependency constraints and workspace dependencies for consistency.

### Guidelines:
- **Check workspace dependencies first**: Before adding a dependency, check if it's already in the workspace
- **Consider workspace-wide usage**: If a dependency might be used by multiple crates, add it to the workspace
- **Use workspace versions**: Prefer `dep = { workspace = true }` over specifying versions in individual crates
- Each crate may have its own `deny.toml` for specific constraints
- Runtime-agnostic crates must ban runtime-specific dependencies
- Check dependencies with `cargo deny check` before commits
- Document why specific dependencies are denied

### Adding Dependencies:
```toml
# First check Cargo.toml [workspace.dependencies]
# If it exists there:
thiserror = { workspace = true }

# If not, consider if it should be workspace-wide
# If yes, add to workspace first, then use workspace = true
# If no (crate-specific), add with version:
special-crate = "1.0"
```

### Installation:
```bash
cargo install cargo-deny
```

## Module Organization

Avoid re-exports from the crate root.

### Guidelines:
- Do not use `pub use` in lib.rs
- Users import types from their defining modules
- This makes the API surface clearer
- Dependencies and versions are explicit

### Example:
```rust
// lib.rs - Good
pub mod error;
pub mod command;
pub mod executor;

// User code - Good
use command_executor::error::Error;
use command_executor::command::Command;

// lib.rs - Bad
pub use crate::error::Error;  // Don't do this
```

## Documentation

All public APIs must be documented.

### Requirements:
- Use `#![warn(missing_docs)]` in all library crates
- Write usage-focused documentation
- Include examples in doc comments
- Document:
  - Panics with `# Panics`
  - Errors with `# Errors`
  - Safety requirements with `# Safety`

### Example:
```rust
/// Executes a command in the specified context.
/// 
/// # Examples
/// ```
/// let output = executor.execute(Command::new("echo").arg("hello")).await?;
/// ```
/// 
/// # Errors
/// Returns `CommandError::SpawnFailed` if the process cannot be started.
pub async fn execute(&self, cmd: Command) -> Result<Output> {
    // ...
}
```

## Testing

Write tests that work across different async runtimes.

### Guidelines:
- Use `smol_potat::test` as the test annotation
- Test both success and error paths
- Keep tests focused and isolated
- Mock external dependencies
- Use property-based testing where appropriate

### Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[smol_potat::test]
    async fn test_command_builder() {
        let cmd = Command::new("echo").arg("test");
        // ...
    }
}
```

## Memory Efficiency

Prefer zero-copy and allocation-free APIs where reasonable.

### Guidelines:
- Avoid unnecessary allocations in hot paths
- Use `&str` instead of `String` for parameters when ownership isn't needed
- Return borrowed data when possible
- Let higher layers decide if they need to allocate

### Example:
```rust
// Good - no allocation, caller decides
pub trait LogFilter {
    fn filter<'a>(&self, line: &'a str) -> Option<&'a str>;
}

// Bad - forces allocation even if caller doesn't need it
pub trait LogFilter {
    fn filter(&self, line: &str) -> Option<String>;
}
```

## Feature Flags

Use feature flags for optional functionality.

### Guidelines:
- Default features should be minimal
- Name features clearly
- Document each feature in Cargo.toml
- Ensure all feature combinations compile
- Use `cfg` attributes consistently

### Example:
```toml
[features]
default = []
ssh = ["dep:openssh"]
docker = ["dep:docker-api"]  # When we find a runtime-agnostic option

# Document features
# - `ssh`: Enables SSH backend for remote execution
# - `docker`: Enables Docker backend for container execution
```
