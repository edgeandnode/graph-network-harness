# ADR: Command Executor Crate

## Status
Proposed

## Context

The graph-network-harness needs to execute commands in various contexts:
- Local processes on the harness host
- Remote processes via SSH
- Inside Docker containers
- Potentially other contexts (WSL, VMs, etc.)

Currently, each execution context would require different APIs and error handling, leading to code duplication and complexity.

## Decision

Create a dedicated `command-executor` crate that provides a unified, type-safe API for executing commands across different contexts.

### Design

Use a generic `Executor<T>` pattern where `T` represents the execution backend:

```rust
pub struct Executor<T> {
    backend: T,
}
```

With marker types for each backend:
- `Local` - Execute on the host machine
- `Ssh` - Execute on remote machines via SSH
- `Docker` - Execute inside Docker containers

### Key Features

1. **Unified Command API**
   ```rust
   pub struct Command {
       program: String,
       args: Vec<String>,
       env: HashMap<String, String>,
       working_dir: Option<PathBuf>,
   }
   ```

2. **Builder Pattern**
   ```rust
   Command::new("ls")
       .arg("-la")
       .env("KEY", "value")
       .current_dir("/tmp")
   ```

3. **Streaming Support**
   - Execute and wait for output
   - Spawn and stream stdout/stderr
   - Process management (kill, wait)

4. **Context-Specific Features**
   - SSH: File upload/download
   - Docker: Volume mounting
   - Local: Signal handling

### Implementation Structure

```
crates/command-executor/
├── src/
│   ├── lib.rs          # Public API
│   ├── command.rs      # Command builder
│   ├── executor.rs     # Executor<T> implementation
│   ├── process.rs      # Process trait
│   ├── backends/
│   │   ├── mod.rs
│   │   ├── local.rs    # Local execution
│   │   ├── ssh.rs      # SSH execution
│   │   └── docker.rs   # Docker execution
│   └── error.rs        # Error types
├── tests/
└── Cargo.toml
```

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["process", "io-util"] }
async-trait = "0.1"
openssh = "0.10"  # SSH backend
bollard = "0.15"  # Docker backend
thiserror = "1.0"
anyhow = "1.0"
bytes = "1.0"
futures = "0.3"
```

## Consequences

### Positive

- **Unified API**: Same interface regardless of execution context
- **Type Safety**: Compile-time checking of executor capabilities
- **Testability**: Easy to mock different execution contexts
- **Extensibility**: New backends can be added without changing the API
- **Reusability**: Can be used by other projects

### Negative

- **Additional Abstraction**: One more layer to understand
- **Feature Disparity**: Not all features work in all contexts
- **Dependency Weight**: Brings in SSH and Docker libraries even if not used

### Mitigations

- Use feature flags to make SSH/Docker optional
- Provide clear documentation on context-specific features
- Keep the core API minimal and focused

## Alternatives Considered

1. **Trait-based with dynamic dispatch**: More flexible but less performant
2. **Enum-based executor**: Simpler but less extensible
3. **Separate crates per backend**: More modular but harder to maintain consistent API
4. **Use existing crate**: No existing crate provides this exact abstraction

## Example Usage

```rust
use command_executor::{Executor, Command};

// Local execution
let local = Executor::local();
let output = local.run(Command::new("echo").arg("hello")).await?;

// SSH execution
let ssh = Executor::ssh("192.168.1.100", "user", "~/.ssh/id_rsa").await?;
let output = ssh.run(Command::new("uname").arg("-a")).await?;

// Docker execution
let docker = Executor::docker("my-container").await?;
let output = docker.run(Command::new("ps").arg("aux")).await?;

// Streaming execution
let mut process = ssh.spawn(Command::new("tail").arg("-f").arg("/var/log/syslog")).await?;
let mut stdout = process.stdout().await?;
while let Some(line) = stdout.next().await {
    println!("Log: {}", line?);
}
```

## References

- [openssh crate](https://docs.rs/openssh/) - SSH client library
- [bollard crate](https://docs.rs/bollard/) - Docker API client
- [tokio::process](https://docs.rs/tokio/latest/tokio/process/) - Async process handling