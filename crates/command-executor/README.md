# command-executor

Runtime-agnostic command execution library providing a unified interface for running commands across different execution contexts (local, SSH, Docker, systemd).

## Features

- **Runtime Agnostic**: Works with any async runtime (Tokio, async-std, smol)
- **Nested Launchers**: Compose launchers for complex scenarios (e.g., Docker on remote host via SSH)
- **Multiple Targets**: Execute commands, manage processes, control systemd services, run Docker containers
- **Type-Safe**: Zero-cost abstractions with compile-time guarantees
- **Event Streaming**: Real-time stdout/stderr streaming with buffering

## Overview

The library is built around three core concepts:

1. **Launchers**: Define WHERE commands run (local, SSH, sudo)
2. **Targets**: Define WHAT to run (command, process, container, service)
3. **Executors**: Combine launchers and targets for execution

## Usage

### Basic Command Execution

```rust
use command_executor::{Executor, Target, Command};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an executor for local execution
    let executor = Executor::local("my-service");
    
    // Run a simple command
    let result = executor.execute(
        &Target::Command,
        Command::new("echo").arg("Hello, World!")
    ).await?;
    
    println!("Output: {}", result.output);
    Ok(())
}
```

### SSH Remote Execution

```rust
use command_executor::{
    Executor, Target, Command,
    backends::{local::LocalLauncher, ssh::{SshLauncher, SshConfig}},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create SSH launcher wrapping local launcher
    let ssh_config = SshConfig::new("example.com")
        .with_user("deploy")
        .with_identity_file("/home/user/.ssh/id_rsa");
    
    let ssh_launcher = SshLauncher::new(LocalLauncher, ssh_config);
    let executor = Executor::new("remote-service", ssh_launcher);
    
    // Execute command on remote host
    let result = executor.execute(
        &Target::Command,
        Command::new("hostname")
    ).await?;
    
    println!("Remote hostname: {}", result.output);
    Ok(())
}
```

### Docker Container Execution

```rust
use command_executor::{Executor, Target, Command, target::DockerContainer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executor = Executor::local("docker-test");
    
    // Define a Docker container target
    let container = DockerContainer::new("alpine:latest")
        .with_remove_on_exit(true);
    
    // Run command in container
    let result = executor.execute(
        &Target::DockerContainer(container),
        Command::new("cat").arg("/etc/os-release")
    ).await?;
    
    println!("Container OS: {}", result.output);
    Ok(())
}
```

### Nested Launchers (Docker on Remote Host)

```rust
use command_executor::{
    Executor, Target, Command,
    backends::{local::LocalLauncher, ssh::{SshLauncher, SshConfig}},
    target::DockerContainer,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // SSH to remote host
    let ssh_config = SshConfig::new("docker-host.example.com");
    let remote_launcher = SshLauncher::new(LocalLauncher, ssh_config);
    
    // Docker container on the remote host
    let container = DockerContainer::new("nginx:latest")
        .with_name("remote-nginx");
    
    let executor = Executor::new("remote-docker", remote_launcher);
    let result = executor.execute(
        &Target::DockerContainer(container),
        Command::new("nginx").arg("-v")
    ).await?;
    
    Ok(())
}
```

### Process Management with Event Streaming

```rust
use command_executor::{Executor, Target, Command, ProcessHandle};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executor = Executor::local("streaming-demo");
    
    // Launch a long-running process
    let (mut events, mut handle) = executor.launch(
        &Target::Command,
        Command::new("ping").arg("-c").arg("5").arg("localhost")
    ).await?;
    
    // Stream output in real-time
    while let Some(event) = events.next().await {
        if let Some(data) = event.data {
            print!("{}", data);
        }
    }
    
    // Wait for completion
    let exit_status = handle.wait().await?;
    println!("Process exited with: {:?}", exit_status);
    
    Ok(())
}
```

## Target Types

- **Command**: One-off command execution
- **ManagedProcess**: Process with lifecycle management
- **SystemdService**: Control systemd services
- **SystemdPortable**: Manage systemd-portable services
- **DockerContainer**: Run Docker containers
- **ComposeService**: Manage Docker Compose services
- **ManagedService**: Generic service with custom commands

## Features

- `ssh`: Enable SSH launcher support
- `docker`: Enable Docker target types

## Architecture

The library uses a composable launcher architecture where launchers can wrap other launchers, enabling complex execution scenarios while maintaining type safety and zero runtime overhead.

```
Executor
  └── Launcher (SSH)
       └── Launcher (Local)
            └── Target (DockerContainer)
                 └── Command
```

This design allows for arbitrary nesting of execution contexts while keeping the API simple and consistent.

## License

See the workspace license file.