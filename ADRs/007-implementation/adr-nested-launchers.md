# ADR: Nested Launcher Architecture

## Status
Accepted

## Context

In the initial design, we planned separate launcher implementations for each execution context:
- `LocalLauncher` for local execution
- `DockerLauncher` for Docker containers
- `SshLauncher` for remote execution

This approach led to several problems:
1. **Duplication**: Docker logic would need to be implemented in both `DockerLauncher` (for local Docker) and `SshLauncher` (for remote Docker)
2. **Limited Composability**: No easy way to express "Docker container on remote host" or "SSH through jump host"
3. **Type Explosion**: Each new execution context requires a new launcher type

## Decision

Implement a **nested launcher architecture** where launchers can wrap other launchers to compose execution contexts.

### Core Design

```rust
// Base launcher executes directly
struct LocalLauncher;

// Other launchers wrap an inner launcher
struct SshLauncher<L: Launcher> {
    inner: L,
    config: SshConfig,
}

// Composition via nesting
let local = LocalLauncher;
let remote = SshLauncher::new(local, "remote-host");
let jump = SshLauncher::new(remote, "jump-host");
```

### Key Principles

1. **Launchers define WHERE**: Local, SSH, future: Kubernetes, Cloud
2. **Targets define WHAT**: Command, DockerContainer, SystemdService, etc.
3. **Any target runs anywhere**: Through launcher composition
4. **Static dispatch**: Zero runtime overhead, all resolved at compile time

### Implementation Pattern

Each nested launcher:
1. Receives a command and target
2. Transforms the command for its context
3. Delegates to its inner launcher
4. Passes through event streams and handles unchanged

```rust
impl<L: Launcher> Launcher for SshLauncher<L> {
    async fn launch(&self, target: &Target, cmd: Command) -> Result<...> {
        // Transform: cmd -> ssh host 'cmd'
        let ssh_cmd = Command::new("ssh")
            .arg(&self.config.host)
            .arg(format_command(cmd));
            
        // Delegate to inner launcher
        self.inner.launch(target, ssh_cmd).await
    }
}
```

## Examples

### Local Docker Container
```rust
let launcher = LocalLauncher;
let target = DockerContainer::new("nginx");
launcher.launch(&target, command).await?;
// Executes: docker run nginx <command>
```

### Remote Docker Container
```rust
let launcher = SshLauncher::new(LocalLauncher, "prod-server");
let target = DockerContainer::new("nginx");
launcher.launch(&target, command).await?;
// Executes: ssh prod-server 'docker run nginx <command>'
```

### Docker via Jump Host
```rust
let launcher = SshLauncher::new(
    SshLauncher::new(LocalLauncher, "prod-server"),
    "jump-host"
);
let target = DockerContainer::new("nginx");
launcher.launch(&target, command).await?;
// Executes: ssh jump-host 'ssh prod-server '"'"'docker run nginx <command>'"'"''
```

## Consequences

### Positive

1. **DRY**: Each execution method implemented once
2. **Composable**: Arbitrary nesting depth
3. **Type Safe**: Invalid combinations caught at compile time
4. **Extensible**: New launchers automatically compose
5. **Zero Cost**: Static dispatch, no runtime overhead
6. **Intuitive**: Execution flow matches type structure

### Negative

1. **Complex Types**: Deep nesting creates verbose type signatures
   ```rust
   SshLauncher<SshLauncher<LocalLauncher>>
   ```
   **Mitigation**: Type aliases for common patterns

2. **Learning Curve**: Nested types may be unfamiliar
   **Mitigation**: Good documentation and examples

3. **Escaping Complexity**: Multiple layers need careful quote escaping
   **Mitigation**: Proper escaping utilities

## Alternatives Considered

### 1. Trait Objects
```rust
Box<dyn Launcher>
```
**Rejected**: Runtime overhead, loss of type information

### 2. Enum-based
```rust
enum Launcher {
    Local(LocalLauncher),
    Ssh(SshLauncher),
    Docker(DockerLauncher),
}
```
**Rejected**: Not composable, requires nested enums for composition

### 3. Execution Context Builder
```rust
ExecutionContext::local()
    .via_ssh("host")
    .in_docker("container")
```
**Rejected**: More complex implementation, runtime construction

## Migration Path

1. Move Docker types from `backends/docker.rs` to `target.rs`
2. Delete `DockerLauncher` implementation
3. Update `LocalLauncher` to handle Docker targets
4. Implement `SshLauncher<L>` with nesting support
5. Update tests to use composed launchers

## Future Extensions

The pattern extends naturally to new execution contexts:

```rust
// Kubernetes pods
struct K8sLauncher<L> {
    inner: L,
    namespace: String,
    pod: String,
}

// Cloud VMs  
struct CloudLauncher<L> {
    inner: L,
    provider: CloudProvider,
    instance: String,
}

// Composition works automatically
let cloud_k8s = K8sLauncher::new(
    CloudLauncher::new(LocalLauncher, aws, "instance-1"),
    "default",
    "my-pod"
);
```

## References

- [Original Command Executor ADR](./adr-command-executor.md)
- [Phase 1 Definition of Done](./phase1-definition-of-done.md)
- Rust's zero-cost abstractions
- Decorator pattern in systems programming