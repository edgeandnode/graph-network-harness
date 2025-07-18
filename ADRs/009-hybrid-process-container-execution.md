# ADR-009: Hybrid Process and Container Execution Model

## Status

Proposed

## Context

The current local-network setup runs all services as Docker containers via docker-compose. This approach has limitations:

1. **Development iteration**: Rebuilding containers for code changes is slow
2. **Resource overhead**: Each container adds ~100MB memory overhead
3. **Startup time**: Container creation adds 5-10 seconds per service
4. **Debugging**: Harder to attach debuggers to containerized processes
5. **Platform limitations**: Some services are just binaries that don't need containerization

However, containers provide benefits:
- Consistent environment across platforms
- Easy dependency management
- Process isolation
- Standardized networking

We need a hybrid approach that provides flexibility for different use cases.

## Decision

Implement a unified service orchestration layer that supports multiple runtime types:

1. **Native processes**: Direct execution for development and performance
2. **Docker containers**: For complex dependencies and isolation
3. **Remote processes**: Via SSH for distributed testing
4. **Mixed mode**: Different runtime types can coexist

The orchestrator acts as a supervisor process that manages lifecycle, logging, and networking for all runtime types uniformly.

## Consequences

### Positive Consequences

- **Fast iteration**: Change code and restart without rebuilding containers
- **Flexible resource usage**: Choose runtime based on service needs
- **Better debugging**: Native processes allow direct debugger attachment
- **Unified interface**: Same commands work regardless of runtime type
- **Gradual migration**: Can move services between runtimes easily

### Negative Consequences

- **Complexity**: More runtime types to support and test
- **Environment consistency**: Native processes may behave differently
- **Dependency management**: Must handle binary dependencies explicitly

### Risks

- **Binary compatibility**: Native binaries must work on host OS
- **Resource conflicts**: Services might conflict over ports/resources
- **State management**: Different cleanup requirements per runtime

## Implementation

### Service Definition
```yaml
services:
  # Native process (fast iteration)
  graph-node:
    runtime: process
    binary: /usr/local/bin/graph-node
    env:
      POSTGRES_URL: "postgres://localhost:5432"
  
  # Container (isolation)
  postgres:
    runtime: container
    image: postgres:15
    volumes:
      - ./data:/var/lib/postgresql/data
  
  # Remote process (distributed)
  indexer-agent:
    runtime: remote
    host: 192.168.1.100
    binary: /opt/indexer/bin/indexer-agent
```

### Runtime Abstraction
```rust
pub trait ServiceRuntime {
    async fn start(&mut self, config: &ServiceConfig) -> Result<ServiceHandle>;
    async fn stop(&mut self, handle: &ServiceHandle) -> Result<()>;
    async fn logs(&self, handle: &ServiceHandle, lines: usize) -> Result<String>;
    async fn health_check(&self, handle: &ServiceHandle) -> Result<HealthStatus>;
}

pub struct ProcessRuntime;
pub struct ContainerRuntime { docker: Docker }
pub struct RemoteRuntime { sessions: HashMap<String, SshSession> }
```

### Unified Process Supervisor
```rust
pub struct ServiceOrchestrator {
    runtimes: HashMap<RuntimeType, Box<dyn ServiceRuntime>>,
    services: HashMap<String, RunningService>,
    network_manager: ServiceMesh,
}

impl ServiceOrchestrator {
    pub async fn start_service(&mut self, def: &ServiceDef) -> Result<()> {
        // 1. Validate runtime requirements
        self.validate_runtime(&def)?;
        
        // 2. Set up networking
        let network_info = self.network_manager.setup_service(&def).await?;
        
        // 3. Prepare environment
        let config = self.prepare_config(&def, network_info)?;
        
        // 4. Start via appropriate runtime
        let runtime = self.runtimes.get_mut(&def.runtime_type)?;
        let handle = runtime.start(&config).await?;
        
        // 5. Track running service
        self.services.insert(def.name.clone(), RunningService {
            definition: def.clone(),
            handle,
            started_at: Utc::now(),
        });
        
        Ok(())
    }
}
```

### Implementation Steps

- [ ] Create ServiceRuntime trait and implementations
- [ ] Implement ProcessRuntime with proper signal handling
- [ ] Add ContainerRuntime using bollard/docker API
- [ ] Create RemoteRuntime with SSH-based execution
- [ ] Build unified logging system across runtimes
- [ ] Add health checking for all runtime types
- [ ] Implement graceful shutdown and cleanup
- [ ] Create runtime-specific configuration validation

## Alternatives Considered

### Alternative 1: Containers only
- Description: Keep current Docker-only approach
- Pros: Simple, consistent environment
- Cons: Slow iteration, resource overhead
- Why rejected: Doesn't meet development speed requirements

### Alternative 2: Processes only
- Description: Run everything as native processes
- Pros: Fast, simple
- Cons: No isolation, platform-specific issues
- Why rejected: Some services need container isolation

### Alternative 3: Separate tools
- Description: Use docker-compose for containers, systemd for processes
- Pros: Use existing tools
- Cons: No unified interface, complex orchestration
- Why rejected: Want single tool for all services

### Alternative 4: Kubernetes
- Description: Use k8s with different runtimes
- Pros: Production-ready, lots of features
- Cons: Complex setup, overkill for development
- Why rejected: Too complex for test harness use case