# ADR: Command Executor Crate

## Status
Accepted (Updated to nested launcher architecture)

## Context

The graph-network-harness needs to execute commands in various contexts:
- Local processes on the harness host
- Remote processes via SSH
- Inside Docker containers
- Potentially other contexts (WSL, VMs, Kubernetes, etc.)

Currently, each execution context would require different APIs and error handling, leading to code duplication and complexity.

## Decision

Create a dedicated `command-executor` crate that provides a unified, type-safe API for executing commands across different contexts using a **composable nested launcher architecture**.

### Design

Use nested launchers that can wrap other launchers to build execution contexts:

```rust
// Base launcher
struct LocalLauncher;

// SSH launcher wraps another launcher
struct SshLauncher<L: Launcher> {
    inner: L,
    config: SshConfig,
}

// Examples of composition:
let local = LocalLauncher;
let remote = SshLauncher::new(LocalLauncher, "remote-host");
let jump = SshLauncher::new(
    SshLauncher::new(LocalLauncher, "target-host"),
    "jump-host"
);
```

This allows arbitrary composition of execution contexts with static dispatch and zero runtime overhead.

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

2. **Composable Launchers**
   ```rust
   // Any launcher can wrap any other launcher
   impl<L: Launcher> Launcher for SshLauncher<L> {
       async fn launch(&self, target: &Target, cmd: Command) -> Result<...> {
           // Transform command for SSH
           let ssh_cmd = wrap_for_ssh(cmd);
           // Delegate to inner launcher
           self.inner.launch(target, ssh_cmd).await
       }
   }
   ```

3. **Location-Agnostic Targets**
   ```rust
   pub enum ExecutionTarget {
       Command(Command),              // One-off command
       ManagedProcess(ManagedProcess), // Process with lifecycle
       DockerContainer(DockerContainer), // Docker container
       SystemdService(SystemdService),   // Systemd service
   }
   ```

4. **Streaming Support**
   - Execute and wait for output
   - Spawn and stream stdout/stderr
   - Process management (kill, wait)
   - Event streams pass through unchanged

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
# Runtime-agnostic async primitives
async-process = "2.0"  # For spawning processes
async-trait = "0.1"
futures = "0.3"
futures-lite = "2.0"

# Error handling
thiserror = "1.0"

# NO runtime-specific dependencies!
# No tokio, no async-std
# SSH and Docker handled via CLI commands
```

## Consequences

### Positive

- **Unified API**: Same interface regardless of execution context
- **Type Safety**: Compile-time checking with zero runtime overhead
- **Composability**: Arbitrary nesting of execution contexts
- **Runtime Agnostic**: Works with any async runtime (smol, tokio, async-std)
- **Zero Cost**: Static dispatch means no performance penalty
- **Extensibility**: New launchers compose with existing ones

### Negative

- **Complex Types**: Deep nesting creates complex type signatures
- **Learning Curve**: Nested launcher concept may be unfamiliar
- **CLI Dependency**: Relies on SSH/Docker CLI tools being available

### Mitigations

- Type aliases for common combinations
- Clear examples and documentation
- Runtime checks for CLI tool availability

## Alternatives Considered

1. **Separate Backend Types**: Original design with `LocalLauncher`, `DockerLauncher`, `SshLauncher` as separate types
   - **Rejected**: Led to duplication (Docker logic in both local and SSH contexts)

2. **Dynamic Dispatch**: Using trait objects for launchers
   - **Rejected**: Runtime overhead and loss of type safety

3. **Builder Chain**: `ExecutionContext::local().via_ssh().in_docker()`
   - **Rejected**: More complex implementation, less flexible than nested types

4. **Use Existing Crates**: openssh, bollard
   - **Rejected**: Require specific runtimes (Tokio), violating our runtime-agnostic requirement

## Example Usage

```rust
use command_executor::{LocalLauncher, SshLauncher, Launcher, Command};
use command_executor::{DockerContainer, ExecutionTarget};

// Local execution
let local = LocalLauncher;
let (events, handle) = local.launch(
    &ExecutionTarget::Command,
    Command::new("echo").arg("hello")
).await?;

// SSH execution
let remote = SshLauncher::new(LocalLauncher, "remote-host");
let (events, handle) = remote.launch(
    &ExecutionTarget::Command,
    Command::new("uname").arg("-a")
).await?;

// Docker on local
let local = LocalLauncher;
let target = ExecutionTarget::DockerContainer(
    DockerContainer::new("nginx:latest")
);
let (events, handle) = local.launch(&target, Command::new("ps")).await?;

// Docker on remote (composition!)
let remote = SshLauncher::new(LocalLauncher, "remote-host");
let target = ExecutionTarget::DockerContainer(
    DockerContainer::new("nginx:latest")
);
let (events, handle) = remote.launch(&target, Command::new("ps")).await?;
// This executes: ssh remote-host 'docker run nginx ps'

// SSH jump host
let jump = SshLauncher::new(
    SshLauncher::new(LocalLauncher, "target"),
    "jump-host"
);
let (events, handle) = jump.launch(
    &ExecutionTarget::Command,
    Command::new("hostname")
).await?;
// This executes: ssh jump-host 'ssh target hostname'
```

## References

- [async-process](https://docs.rs/async-process/) - Runtime-agnostic process spawning
- [smol](https://docs.rs/smol/) - Lightweight async runtime for testing
- [Command Executor Design](./adr-nested-launchers.md) - Detailed nested launcher architecture