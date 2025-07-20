# Phase 1: Command Executor - Definition of Done

## Objective
Create a standalone `command-executor` crate that provides type-safe, unified command execution using a composable nested launcher architecture that works across any execution context (Local, SSH, Docker, etc.) with proper event streaming integration.

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

### 4. Launcher/Attacher Traits ✓ When:
- [x] `Backend` trait renamed to `Launcher` trait
- [x] `Launcher` trait uses associated types for EventStream and Handle
- [x] No unnecessary dynamic dispatch
- [x] `execute()` and `launch()` methods are implemented
- [x] `Attacher` trait created for connecting to existing services
- [x] `AttachedHandle` trait for service lifecycle control
- [x] Proper error handling with custom error types

### 5. Local Launcher Implementation ✓ When:
- [x] `LocalLauncher` fully implements Launcher trait
- [x] Handles LaunchedTarget types:
- [x] Command: Direct process spawning with async-process
- [x] ManagedProcess: Process lifecycle management
- [x] SystemdPortable: Portable service attachment/start
- [x] Concurrent stdout/stderr reading
- [x] Signal handling on Unix (via nix crate)
- [x] Graceful degradation on Windows
- [x] Process cleanup on drop

### 6. Local Attacher Implementation ✓ When:
- [x] `LocalAttacher` implements Attacher trait
- [x] Handles AttachedTarget types:
- [x] ManagedService: Generic service with configurable commands
- [x] Service status checking
- [x] Service lifecycle control (start/stop/restart/reload)
- [x] Log streaming via configurable commands
- [x] Support for systemd, rc.d, and custom services

### 7. Nested Launcher Architecture ✓ When:
- [ ] Launchers can wrap other launchers (e.g., `SshLauncher<LocalLauncher>`)
- [ ] Docker is a target type, not a launcher
- [ ] LocalLauncher handles Docker targets:
  - [ ] DockerContainer: `docker run` commands
  - [ ] ComposeService: `docker-compose` commands
  - [ ] Container lifecycle management
- [ ] Static dispatch with zero runtime overhead
- [ ] Type-safe composition of execution contexts

### 8. SSH Launcher Implementation ✓ When:
- [ ] `SshLauncher<L>` wraps any other launcher
- [ ] Uses SSH CLI via async-process (runtime-agnostic)
- [ ] Transforms commands for SSH execution
- [ ] Supports nested SSH (jump hosts)
- [ ] Key-based authentication support
- [ ] Handles all target types via delegation

### 9. Process Management ✓ When:
- [x] `ProcessHandle` trait with event streaming for launched processes
- [x] `AttachedHandle` trait for attached service control
- [x] Signal methods work appropriately for each handle type:
  - [x] Local launched: Unix signals via nix
  - [x] Local attached: Service control commands
  - [x] Docker via LocalLauncher: docker stop/kill commands
  - [ ] SSH wrapped: signals sent via SSH
- [x] `wait()` for process completion
- [x] `terminate()` and `kill()` convenience methods
- [x] Event streams and handles are separate from spawn/attach

### 10. Comprehensive Testing ✓ When:

#### `tests/local_launcher.rs`
- [x] Basic command execution (echo, cat) - runs on host
- [x] Long-running processes
- [x] Signal handling (SIGTERM, SIGKILL)
- [x] Environment variables
- [x] Working directory
- [x] Error cases (command not found)
- [x] ManagedProcess lifecycle (PID tracking, restart)

#### `tests/local_attacher.rs`
- [x] ManagedService with systemd commands
- [x] Service status checking
- [x] Service lifecycle control (start/stop/restart)
- [x] Log streaming from services
- [x] Custom service command configuration

#### `tests/systemd_integration.rs`
- [x] Create Docker container with systemd (e.g., using systemd/systemd image)
- [x] Test ManagedService with systemd commands:
  - [x] Start/stop/restart services via attacher
  - [x] Status checking
  - [x] Journal log streaming
- [x] Test SystemdPortable via launcher:
  - [x] Attach/detach portable services
  - [x] Start/stop portable services
  - [x] List attached services
- [x] Cleanup container after tests
**Note:** Tests are implemented but marked with `#[ignore]` until SSH/Docker infrastructure is ready for remote execution.

#### `tests/nested_launchers.rs`
- [ ] Test LocalLauncher with Docker targets
- [ ] Test SshLauncher wrapping LocalLauncher
- [ ] Test Docker on remote (SSH<Local> + DockerContainer)
- [ ] Test SSH jump hosts (SSH<SSH<Local>>)
- [ ] Test command transformation through layers
- [ ] Verify proper cleanup through nesting

#### `tests/docker_targets.rs`
- [ ] LocalLauncher + DockerContainer target
- [ ] LocalLauncher + ComposeService target
- [ ] Environment variable injection
- [ ] Volume mounts
- [ ] Container lifecycle management

#### `tests/ssh_launcher.rs`
- [ ] Basic SSH command execution
- [ ] SSH with key authentication
- [ ] Nested SSH (jump host scenarios)
- [ ] All target types via SSH
- [ ] Connection error handling

### 11. Examples ✓ When:
- [ ] `examples/basic_launch.rs` - Simple command execution
- [ ] `examples/nested_launchers.rs` - Composition examples (SSH<Local>, etc.)
- [ ] `examples/docker_targets.rs` - Docker containers via any launcher
- [ ] `examples/service_attach.rs` - Service attachment and control
- [ ] `examples/streaming_logs.rs` - Event streaming
- [ ] `examples/jump_host.rs` - SSH jump host scenarios
- [ ] Examples have comments explaining usage
- [ ] Examples actually run and produce expected output

### 12. Documentation ✓ When:
- [ ] All public types have doc comments
- [ ] Module-level documentation explains nested launcher architecture
- [ ] Examples showing composition patterns
- [ ] README.md explains the design philosophy
- [ ] Target types vs Launcher types clearly documented
- [ ] No missing_docs warnings

### 13. API Quality ✓ When:
- [ ] API is ergonomic (easy to use correctly)
- [ ] Hard to misuse (type safety)
- [ ] Good error messages
- [ ] Consistent naming across launchers and attachers
- [ ] Launcher/Attacher-specific features are discoverable
- [ ] Clear distinction between launch vs attach semantics
- [ ] Follows Rust conventions

## Success Criteria

Phase 1 is complete when:
1. All checkboxes above are checked
2. Nested launcher architecture is fully functional (Local, SSH<L>, composition)
3. Test coverage demonstrates real-world usage:
   - Local process launching works
   - Local service attachment works
   - Docker container launching works
   - Docker compose service launching works
   - SSH remote launching works
4. The following examples work:

   **Launching processes:**
   ```rust
   // Local execution
   let local = LocalLauncher;
   let (events, handle) = local.launch(&target, command).await?;
   
   // Remote execution (nested)
   let remote = SshLauncher::new(LocalLauncher, "host");
   let (events, handle) = remote.launch(&target, command).await?;
   
   // Docker on remote (composition)
   let docker_target = ExecutionTarget::DockerContainer(
       DockerContainer::new("nginx")
   );
   let (events, handle) = remote.launch(&docker_target, command).await?;
   ```

   **Attaching to services:**
   ```rust
   // Service attachment and control
   let (events, handle) = attacher.attach(&service, config).await?;
   
   // Monitor service and control as needed
   while let Some(event) = events.next().await {
       if should_restart(&event) {
           handle.restart().await?;
       }
   }
   ```
5. PR is reviewed and merged

## Recent Enhancements

### Command API
- Introduced a reusable `Command` struct with builder pattern
- Replaced direct use of `AsyncCommand` throughout the codebase
- Provides cloneable, configurable command building
- Method `prepare()` converts to `AsyncCommand` when needed

### API Improvements
- Made all struct fields private for proper encapsulation
- Added constructors and builders:
  - `ManagedProcess::new()` and `ManagedProcess::builder()`
  - `SystemdService::new()`
  - `SystemdPortable::new()`
  - `ManagedService::builder()` with comprehensive builder pattern
- Added necessary getter methods for accessing private fields
- All tests updated to use the new Command API

## Non-Goals (Future Phases)
- Integration with service registry
- Package management
- WireGuard networking
- Kubernetes launcher/attacher
- Complex service orchestration
