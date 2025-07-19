# Phase 1: Command Executor - Definition of Done

## Objective
Create a standalone `command-executor` crate that provides type-safe, unified command execution across Local, Docker, and SSH contexts with proper event streaming integration.

## Deliverables

### 1. Crate Structure ✓ When:
- [ ] `crates/command-executor/` exists with proper Cargo.toml
- [ ] All required dependencies are specified (including openssh, bollard)
- [ ] Feature flags for optional backends (ssh, docker) are defined
- [ ] Crate builds without warnings

### 2. Core Types ✓ When:
- [ ] `Executor<T>` struct is implemented with service_name tracking
- [ ] `Command` builder supports:
  - [ ] Program and arguments
  - [ ] Environment variables
  - [ ] Working directory
  - [ ] Builder pattern with method chaining
- [ ] `Signal` enum covers common Unix signals
- [ ] `ExecutionResult` returns event stream + exit status handle
- [ ] `ExecutionTarget` enum supports:
  - [ ] Command (one-off execution)
  - [ ] ManagedProcess (we manage lifecycle)
  - [ ] SystemdService (systemctl commands)
  - [ ] SystemdPortable (portablectl commands)
  - [ ] DockerContainer (docker run)
  - [ ] ComposeService (docker-compose commands)

### 3. Event Integration ✓ When:
- [ ] Uses existing `ServiceEvent` from harness crate
- [ ] Stdout/stderr are converted to proper `ServiceEvent` instances
- [ ] Process lifecycle events (Started, Stopped, Crashed) are generated
- [ ] Structured log parsing (JSON) creates rich events with data
- [ ] Event severity is properly assigned (stderr → Warning, etc.)

### 4. Backend Trait ✓ When:
- [ ] `Backend` trait uses associated type for Process
- [ ] No unnecessary dynamic dispatch
- [ ] `execute()` and `spawn()` methods are implemented
- [ ] Proper error handling with custom error types

### 5. Local Backend Implementation ✓ When:
- [ ] `LocalBackend` fully implements Backend trait
- [ ] Handles all ExecutionTarget types:
- [ ] Command: Direct process spawning with tokio::process
- [ ] Concurrent stdout/stderr reading
- [ ] Signal handling on Unix (via nix crate)
- [ ] Graceful degradation on Windows
- [ ] Process cleanup on drop

### 6. Docker Backend Implementation ✓ When:
- [ ] `DockerBackend` implements Backend trait
- [ ] Can execute in existing containers
- [ ] Can execute in compose service containers
- [ ] Handles container stdout/stderr streaming
- [ ] Converts Docker events to ServiceEvents
- [ ] Supports exec with environment variables

### 7. SSH Backend Implementation ✓ When:
- [ ] `SshBackend` implements Backend trait
- [ ] Connection management with key-based auth
- [ ] Remote command execution
- [ ] Remote process management
- [ ] File upload/download capabilities
- [ ] Handles connection failures gracefully

### 8. Process Management ✓ When:
- [ ] `Process` trait with event streaming for all backends
- [ ] `signal()` method works appropriately for each backend:
  - [ ] Local: Unix signals via nix
  - [ ] Docker: docker kill with signal support
  - [ ] SSH: remote kill command
- [ ] `wait()` for process completion
- [ ] `terminate()` and `kill()` convenience methods
- [ ] Events can only be taken once (ownership transfer)

### 9. Comprehensive Testing ✓ When:

#### `tests/local_executor.rs`
- [ ] Basic command execution (echo, cat) - runs on host
- [ ] Long-running processes
- [ ] Signal handling (SIGTERM, SIGKILL)
- [ ] Environment variables
- [ ] Working directory
- [ ] Error cases (command not found)
- [ ] ManagedProcess lifecycle (PID tracking, restart)

#### `tests/systemd_executor.rs`
- [ ] Create Docker container with systemd (e.g., using systemd/systemd image)
- [ ] Test SystemdService execution:
  - [ ] Start/stop/restart services
  - [ ] Status checking
  - [ ] Enable/disable services
  - [ ] Journal log streaming
- [ ] Test SystemdPortable execution:
  - [ ] Attach/detach portable services
  - [ ] Start/stop portable services
  - [ ] List attached services
- [ ] Cleanup container after tests

#### `tests/docker_executor.rs`
- [ ] Create test container
- [ ] Execute commands in container
- [ ] Environment variable injection
- [ ] Volume mounts
- [ ] Container cleanup

#### `tests/compose_executor.rs`
- [ ] Create test docker-compose.yml
- [ ] Start compose stack
- [ ] Execute in specific service
- [ ] Access service by name
- [ ] Stack cleanup

#### `tests/ssh_executor.rs`
- [ ] Create Docker container with sshd
- [ ] Generate test SSH keys
- [ ] Connect and execute commands
- [ ] File upload/download
- [ ] Long-running commands over SSH
- [ ] Connection error handling

### 10. Examples ✓ When:
- [ ] `examples/basic_exec.rs` - Simple command execution (all backends)
- [ ] `examples/streaming_logs.rs` - Event streaming from long process
- [ ] `examples/process_control.rs` - Start, signal, kill
- [ ] `examples/docker_exec.rs` - Docker-specific features
- [ ] `examples/ssh_exec.rs` - SSH with file transfer
- [ ] Examples have comments explaining usage
- [ ] Examples actually run and produce expected output

### 11. Documentation ✓ When:
- [ ] All public types have doc comments
- [ ] Module-level documentation explains design
- [ ] Examples in doc comments
- [ ] README.md in crate root
- [ ] Backend-specific features documented
- [ ] No missing_docs warnings

### 12. API Quality ✓ When:
- [ ] API is ergonomic (easy to use correctly)
- [ ] Hard to misuse (type safety)
- [ ] Good error messages
- [ ] Consistent naming across backends
- [ ] Backend-specific features are discoverable
- [ ] Follows Rust conventions

## Success Criteria

Phase 1 is complete when:
1. All checkboxes above are checked
2. All three backends (Local, Docker, SSH) are fully functional
3. Test coverage demonstrates real-world usage:
   - Local process execution works
   - Docker container execution works
   - Docker compose service execution works
   - SSH to a test container works
4. The following example works for all backends:
   ```rust
   // Local
   let executor = Executor::local("my-service");
   
   // Docker
   let executor = Executor::docker("my-service", "container-id").await?;
   
   // SSH
   let executor = Executor::ssh("my-service", "localhost", "user", key_path).await?;
   
   // All use same API
   let mut result = executor.execute(
       Command::new("echo").arg("hello")
   ).await?;
   
   while let Some(event) = result.events.recv().await {
       println!("{:?}", event);
   }
   ```
5. PR is reviewed and merged

## Non-Goals (Future Phases)
- Complex ExecutionTarget types (systemd services, etc.)
- Integration with service registry
- Package management
- WireGuard networking
