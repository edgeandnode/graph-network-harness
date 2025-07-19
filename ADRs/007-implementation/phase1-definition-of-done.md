# Phase 1: Command Executor - Definition of Done

## Objective
Create a standalone `command-executor` crate that provides type-safe, unified command execution across Local, Docker, and SSH contexts with proper event streaming integration.

## Deliverables

### 1. Crate Structure ✓ When:
- [x] `crates/command-executor/` exists with proper Cargo.toml
- [x] All required dependencies are specified (openssh included, bollard pending)
- [x] Feature flags for optional backends (ssh, docker) are defined
- [x] Crate builds without warnings

### 2. Core Types ✓ When:
- [x] `Executor<T>` struct is implemented with service_name tracking
- [x] Using async_process::Command directly (runtime-agnostic)
  - [x] Program and arguments
  - [x] Environment variables
  - [x] Working directory
  - [x] Builder pattern with method chaining
- [x] Signal methods on Process trait (terminate, kill, interrupt, reload)
- [x] Process trait returns event stream + exit status
- [x] Target types as individual structs per backend:
  - [x] Command (one-off execution)
  - [x] ManagedProcess (we manage lifecycle)
  - [x] SystemdService (systemctl commands)
  - [x] SystemdPortable (portablectl commands)
  - [x] DockerContainer (docker run)
  - [x] ComposeService (docker-compose commands)

### 3. Event Integration ✓ When:
- [x] Uses ProcessEvent (simpler than ServiceEvent, harness will convert)
- [x] Stdout/stderr are converted to ProcessEvent instances
- [x] Process lifecycle events (Started, Exited) are generated
- [x] Line buffering for stdout/stderr
- [x] LogFilter trait for optional filtering

### 4. Backend Trait ✓ When:
- [x] `Backend` trait uses associated type for Process
- [x] No unnecessary dynamic dispatch
- [x] `execute()` and `spawn()` methods are implemented
- [x] Proper error handling with custom error types

### 5. Local Backend Implementation ✓ When:
- [x] `LocalBackend` fully implements Backend trait
- [ ] Handles all ExecutionTarget types:
- [x] Command: Direct process spawning with async-process
- [x] Concurrent stdout/stderr reading
- [x] Signal handling on Unix (via nix crate)
- [x] Graceful degradation on Windows
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
- [x] `Process` trait with event streaming for all backends
- [x] `signal()` method works appropriately for each backend:
  - [x] Local: Unix signals via nix
  - [ ] Docker: docker kill with signal support
  - [ ] SSH: remote kill command
- [x] `wait()` for process completion
- [x] `terminate()` and `kill()` convenience methods
- [x] Events can only be taken once (ownership transfer)

### 9. Comprehensive Testing ✓ When:

#### `tests/local_executor.rs`
- [x] Basic command execution (echo, cat) - runs on host
- [x] Long-running processes
- [x] Signal handling (SIGTERM, SIGKILL)
- [x] Environment variables
- [x] Working directory
- [x] Error cases (command not found)
- [x] ManagedProcess lifecycle (PID tracking, restart)

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
