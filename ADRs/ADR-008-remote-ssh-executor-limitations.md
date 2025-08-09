# ADR-008: RemoteSSH Executor Layer Composition Limitations

## Status
Superseded - LayeredServiceExecutor implemented to address limitations

## Context
The RemoteSSH executor was implemented to support running services on remote hosts via SSH. However, the current implementation hard-codes the use of `LayeredExecutor::new(LocalLauncher).with_layer(ssh_layer)`, limiting its flexibility.

## Problem
Users may need more complex remote execution scenarios:
- Multi-hop SSH through jump/bastion hosts
- Running Docker containers on remote hosts (SSH + Docker layers)
- Using custom launchers instead of LocalLauncher
- Chaining multiple execution contexts

Currently, these scenarios are not supported because the executor only creates a single SSH layer on top of LocalLauncher.

## Decision
For the initial implementation, we will:
1. Keep the RemoteSSH executor simple with single-layer SSH execution
2. Document the limitation clearly
3. Design a path forward for future enhancement

## Consequences

### Positive
- Simple, working implementation for basic SSH use cases
- Clear separation of concerns
- Easy to test and understand
- Covers the most common use case (direct SSH)

### Negative
- Cannot support complex multi-layer scenarios
- Users needing SSH + Docker must work around the limitation
- May need significant refactoring to add flexibility later

## Future Options

### Option 1: Recursive ServiceTarget
Allow ServiceTarget::Remote to contain another ServiceTarget:
```rust
pub enum RemoteMode {
    Process { binary: String, args: Vec<String> },
    Target(Box<ServiceTarget>), // Recursive target
}
```

### Option 2: Explicit Layer Configuration
Add a new ServiceTarget variant for explicit layer composition:
```rust
pub enum ServiceTarget {
    Layered {
        layers: Vec<ExecutionLayer>,
        command: Command,
    },
    // ... existing variants
}
```

### Option 3: Extend RemoteMode
Make RemoteMode more sophisticated:
```rust
pub enum RemoteMode {
    Process { binary: String, args: Vec<String> },
    Docker { image: String, command: Vec<String> },
    MultiHop { next_hop: RemoteConfig, mode: Box<RemoteMode> },
}
```

### Option 4: Factory Pattern
Create an ExecutorFactory that can build complex executors from configuration:
```rust
trait ExecutorFactory {
    fn build_executor(&self, config: &ServiceConfig) -> Box<dyn ServiceExecutor>;
}
```

## Recommendation
Start with the current simple implementation and gather user feedback. If complex remote execution becomes a common need, implement Option 2 (Explicit Layer Configuration) as it provides the most flexibility while maintaining clarity in configuration.

## Resolution
**IMPLEMENTED**: The LayeredServiceExecutor has been implemented following Option 2 (Explicit Layer Configuration). This provides:

- Flexible composition of execution layers (Local, SSH, Docker)
- Support for multi-hop SSH and SSH + Docker scenarios
- Clear YAML configuration via `ServiceTarget::Layered`
- Backward compatibility with existing RemoteSSH executor

The LayeredServiceExecutor addresses all the limitations identified in this ADR while maintaining the simplicity of single-layer executors for basic use cases.