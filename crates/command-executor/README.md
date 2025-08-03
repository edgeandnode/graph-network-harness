# command-executor

Runtime-agnostic command execution library providing layered execution and service attachment capabilities.

## Features

- **Runtime Agnostic**: Works with any async runtime (Tokio, async-std, smol)
- **Layered Execution**: Compose execution layers for complex scenarios (SSH + Docker)
- **Dual Backend System**: Launchers for new processes, Attachers for existing services
- **Stdin Support**: Channel-based stdin forwarding through all layers
- **Type-Safe Handles**: Different handle types for managed vs attached services
- **Event Streaming**: Real-time stdout/stderr streaming with typed events

## Overview

The library is built around these core concepts:

1. **Launchers**: Create new processes with full lifecycle control
2. **Attachers**: Connect to existing services with read-only access
3. **Layers**: Transform commands for different execution contexts
4. **Handles**: Type-safe process management interfaces

## Architecture

```
┌─────────────────────────────────────────┐
│            Application                   │
├─────────────────────────────────────────┤
│         LayeredExecutor                  │
│    ┌─────────────────────────┐         │
│    │   Execution Layers      │         │
│    ├─────────────────────────┤         │
│    │ • SshLayer              │         │
│    │ • DockerLayer           │         │
│    │ • LocalLayer            │         │
│    └───────────┬─────────────┘         │
├────────────────┼───────────────────────┤
│                ▼                        │
│    ┌───────────────────────┐           │
│    │      Backends         │           │
│    ├───────────────────────┤           │
│    │ • LocalLauncher       │           │
│    │ • LocalAttacher       │           │
│    └───────────────────────┘           │
└─────────────────────────────────────────┘
```

## Usage

### Basic Command Execution

```rust
use command_executor::{Command, backends::LocalLauncher, launcher::Launcher, target::Target};

#[smol_potat::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let launcher = LocalLauncher;
    
    // Simple command execution
    let result = launcher.execute(
        &Target::Command,
        Command::new("echo").arg("Hello, World!")
    ).await?;
    
    println!("Output: {}", result.output);
    Ok(())
}
```

### Layered Execution (SSH + Docker)

```rust
use command_executor::{
    layered::{LayeredExecutor, SshLayer, DockerLayer},
    backends::LocalLauncher,
    Command, Target,
};

#[smol_potat::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create executor with stacked layers
    let executor = LayeredExecutor::new(LocalLauncher)
        .with_layer(SshLayer::new("user@remote-host"))
        .with_layer(DockerLayer::new("my-container"));
    
    // This will: SSH to remote-host, then docker exec in my-container
    let (mut events, handle) = executor.execute(
        Command::new("ls").arg("-la"),
        &Target::Command
    ).await?;
    
    // Stream output
    use futures::StreamExt;
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            println!("{}", data);
        }
    }
    
    Ok(())
}
```

### Process Management with Stdin

```rust
use command_executor::{Command, backends::LocalLauncher, launcher::Launcher, target::{Target, ManagedProcess}};
use futures::StreamExt;

#[smol_potat::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let launcher = LocalLauncher;
    
    // Create command with stdin channel
    let (tx, rx) = async_channel::bounded(10);
    let mut command = Command::new("cat");
    command.stdin_channel(rx);
    
    // Launch with process management
    let (mut events, mut handle) = launcher.launch(
        &Target::ManagedProcess(ManagedProcess::new()),
        command
    ).await?;
    
    // Start stdin forwarding
    if let Some(stdin_handle) = handle.take_stdin_for_forwarding() {
        smol::spawn(async move {
            let _ = stdin_handle.forward_channel().await;
        }).detach();
    }
    
    // Send data through stdin
    tx.send("Hello from stdin!".to_string()).await?;
    drop(tx); // Close to signal EOF
    
    // Read output
    while let Some(event) = events.next().await {
        if let Some(data) = &event.data {
            println!("Output: {}", data);
        }
    }
    
    Ok(())
}
```

### Attaching to Existing Services

```rust
use command_executor::{backends::LocalAttacher, attacher::Attacher, target::AttachedService};

#[smol_potat::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let attacher = LocalAttacher;
    
    // Attach to an existing service
    let target = AttachedService {
        name: "my-service".to_string(),
        pid: Some(12345),
    };
    
    let (mut events, handle) = attacher.attach(&target).await?;
    
    // Can only check status, not control lifecycle
    let status = handle.status().await?;
    println!("Service status: {:?}", status);
    
    Ok(())
}
```

## Execution Layers

Layers transform commands without executing them:

```rust
// SSH Layer - wraps command for SSH execution
let ssh_layer = SshLayer::new("user@host")
    .with_port(2222)
    .with_identity_file("/path/to/key");

// Docker Layer - wraps for docker exec
let docker_layer = DockerLayer::new("container")
    .with_interactive(true)
    .with_user("app");

// Local Layer - adds environment variables
let local_layer = LocalLayer::new()
    .with_env("DEBUG", "1");
```

## Target Types

- **Command**: One-off command execution
- **ManagedProcess**: Process with lifecycle management
- **AttachedService**: Existing service (read-only access)
- **ManagedService**: Service with start/stop/restart commands

## Handle Types

- **ProcessHandle**: Full lifecycle control (start, stop, kill, restart)
- **AttachedHandle**: Read-only status monitoring

## Error Handling

The library uses `thiserror` with composable error types:

```rust
use command_executor::error::Error;

match result {
    Err(Error::SpawnFailed(msg)) => println!("Failed to spawn: {}", msg),
    Err(Error::Timeout(duration)) => println!("Timed out after {:?}", duration),
    Err(Error::Io(e)) => println!("IO error: {}", e),
    _ => {}
}
```

## Runtime Agnostic Design

The library uses only runtime-agnostic dependencies:
- `async-process` for process management
- `async-channel` for channels
- `async-trait` for trait definitions
- No direct tokio/async-std usage

## Testing

Run tests with various feature flags:

```bash
# Basic tests
cargo test

# With SSH tests (requires Docker)
cargo test --features ssh-tests

# With Docker tests
cargo test --features docker-tests
```

## License

See the workspace license file.