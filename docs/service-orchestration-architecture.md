# Service Orchestration Architecture

## Overview

The service orchestration system provides heterogeneous service management through a layered architecture that separates execution mechanisms from lifecycle management:

```
┌─────────────────────┐       ┌─────────────────────┐
│   harness CLI       │       │ graph-test-daemon   │
│  (User Interface)   │       │ (Specialized Daemon) │
└──────────┬──────────┘       └──────────┬──────────┘
           │ WebSocket/TLS               │ WebSocket/TLS  
           ▼                             ▼
┌─────────────────────┐       ┌─────────────────────┐
│   harness daemon    │       │  service-registry   │
│  (Server Process)   │◄─────►│  (Service Disco)    │
└──────────┬──────────┘       └─────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────┐
│            service-orchestration                 │
│  ┌─────────────────┐    ┌──────────────────┐   │
│  │ ServiceManager  │───►│ ServiceExecutors │   │
│  └─────────────────┘    └──────────────────┘   │
│                                │                 │
│  ProcessExecutor    DockerExecutor    (SSH*)    │
│         │                  │                     │
└─────────┼──────────────────┼────────────────────┘
          │                  │
          ▼                  ▼
┌─────────────────────────────────────────────────┐
│            command-executor                      │
│  ┌──────────────┐    ┌─────────────────────┐   │
│  │   Backends   │    │  Execution Layers   │   │
│  ├──────────────┤    ├─────────────────────┤   │
│  │ LocalLauncher│    │ LocalLayer          │   │
│  │ LocalAttacher│    │ SshLayer            │   │
│  └──────────────┘    │ DockerLayer         │   │
│                      └─────────────────────┘   │
└─────────────────────────────────────────────────┘

* SshExecutor planned but not yet implemented
```

## Component Responsibilities

### 1. harness CLI (`crates/harness/`)
- **User Interface**: Command-line interface for service management
- **Client Mode**: Connects to daemon via WebSocket over TLS
- **Commands**: start, stop, status, daemon management
- **Protocol**: Request/Response messages serialized as JSON

### 2. harness daemon (`crates/harness/src/daemon/`)
- **Server Process**: Long-running daemon that manages services
- **WebSocket Server**: TLS-secured WebSocket API
- **State Management**: Maintains ServiceManager and Registry instances
- **Request Handling**: Processes client requests and returns responses

#### Specialized Daemons

**graph-test-daemon** (`crates/graph-test-daemon/`) is an example of a specialized daemon:
- **Domain-Specific Services**: Graph Protocol services (PostgreSQL, Anvil, IPFS, Graph Node)
- **YAML Configuration**: Declarative service stack configuration with service type linking
- **Service Type Mapping**: Links YAML `service_type` to action implementations
- **Actionable Services**: Each service provides domain-specific actions (mine-blocks, deploy-subgraph, etc.)
- **Built on harness-core**: Extends BaseDaemon with Graph Protocol semantics

### 3. service-orchestration (`crates/service-orchestration/`)
- **ServiceManager**: Central orchestrator for service lifecycle
- **Service Executors**: Different backends for service execution
  - `ProcessExecutor`: Local process management using command-executor
  - `DockerExecutor`: Docker container management using docker CLI
  - `SshExecutor`: Remote SSH execution (planned, not implemented)
  - `SystemdExecutor`: Systemd service management (planned)
  - `PackageDeployer`: Deploy packages to remote targets (planned)
- **Health Monitoring**: Configurable health checks with retry logic
- **Event Streaming**: Real-time service output via `stream_events` API
- **State Management**: Persistent service state with RwLock synchronization

### 4. command-executor (`crates/command-executor/`)
- **Dual Backend System**:
  - **Launchers**: Create new processes (`LocalLauncher`)
  - **Attachers**: Connect to existing services (`LocalAttacher`)
- **Layered Execution**: Composable command transformation
  - `LocalLayer`: Direct execution with env vars
  - `SshLayer`: SSH command wrapping with forwarding options
  - `DockerLayer`: Docker exec wrapping with interactive mode
- **Process Management**: 
  - `ProcessHandle`: Full lifecycle control (start/stop/kill)
  - `AttachedHandle`: Read-only status monitoring
- **Stdin Support**: Channel-based stdin forwarding
- **Event Streaming**: Typed process events with filtering

### 5. service-registry (`crates/service-registry/`)
- **Service Discovery**: Track running services and their endpoints
- **Network Topology**: Manage service network relationships
- **Persistence**: Store service state across daemon restarts

### 6. harness-core (`crates/harness-core/`)
- **BaseDaemon**: Foundation for building specialized daemons
- **ServiceStack**: Registry for actionable services with JSON-RPC style actions
- **TaskStack**: Registry for one-time deployment tasks
- **Service Traits**: ServiceSetup, StatefulService for lifecycle management
- **DeploymentTask**: One-time operations like contract deployment
- **Runtime Validation**: Validates YAML configurations against registered implementations

## Integration Points

### Service Start Flow

1. **User Command**: `harness start my-service`

2. **CLI Processing**:
   ```rust
   // Parse config and resolve dependencies
   let config = parser::parse_file(config_path)?;
   let ordered_services = dependencies::topological_sort(&config, &services)?;
   
   // Connect to daemon
   let daemon = client::connect_to_daemon().await?;
   
   // Convert config and send request
   let service_config = parser::convert_to_orchestrator_with_context(...)?;
   let request = Request::StartService { name, config: service_config };
   daemon.send_request(request).await?;
   ```

3. **Daemon Handling**:
   ```rust
   // In daemon/handlers.rs
   match request {
       Request::StartService { name, config } => {
           let running_service = state.service_manager.start_service(&name, config).await?;
           Ok(Response::ServiceStarted { name, network_info })
       }
   }
   ```

4. **ServiceManager Orchestration**:
   ```rust
   // In service-orchestration/manager.rs
   pub async fn start_service(&self, name: &str, config: ServiceConfig) -> Result<RunningService> {
       // Select appropriate executor based on target type
       match &config.target {
           ServiceTarget::Process { .. } => self.executors["process"].start(config).await,
           ServiceTarget::Docker { .. } => self.executors["docker"].start(config).await,
           // etc.
       }
   }
   ```

5. **Executor Implementation**:
   ```rust
   // In service-orchestration/executors/process.rs
   pub struct ProcessExecutor {
       executor: Executor<LocalLauncher>,
       health_checker: HealthChecker,
       running_processes: Arc<Mutex<HashMap<String, ProcessInfo>>>,
   }
   
   impl ServiceExecutor for ProcessExecutor {
       async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
           // Extract process-specific config
           let ServiceTarget::Process { binary, args, env, working_dir } = &config.target;
           
           // Build command using command-executor
           let mut cmd = Command::new(binary);
           cmd.args(args);
           for (key, value) in env {
               cmd.env(key, value);
           }
           
           // Launch using ManagedProcess target for lifecycle control
           let target = Target::ManagedProcess(ManagedProcess::new());
           let (event_stream, handle) = self.executor.launch(&target, cmd).await?;
           
           // Store handle for lifecycle management
           let process_info = ProcessInfo {
               handle: Box::new(handle),
               event_stream: SharedEventStream::new(event_stream),
           };
           self.running_processes.lock().await.insert(service_id, process_info);
           
           // Return RunningService with network info
           Ok(RunningService { 
               id: service_id,
               name: config.name,
               pid: Some(pid),
               network_info: Some(NetworkInfo { /* ... */ }),
               start_time: Utc::now(),
           })
       }
   }
   ```

### Key Design Patterns

1. **Layered Architecture**: 
   - Command execution separated from service lifecycle
   - Layers transform commands without executing them
   - Executors handle service-specific lifecycle concerns

2. **Launcher/Attacher Separation**:
   - Launchers create new processes with full control
   - Attachers connect to existing services read-only
   - Different handle types enforce access patterns

3. **Interior Mutability**: 
   - ServiceManager uses `Arc<RwLock<HashMap>>` for state
   - Executors use `Arc<Mutex<HashMap>>` for process tracking
   - Allows `&self` methods as required by traits

4. **Event Streaming**: 
   - Async streams for real-time output
   - SharedEventStream for multiple consumers
   - Typed events (stdout, stderr, exit, etc.)

5. **Runtime Agnostic**: 
   - Uses async-trait for all public APIs
   - No tokio/async-std in library crates
   - Binary uses smol runtime

### Service Lifecycle States

```
┌────────────┐     start()      ┌──────────┐
│ NotStarted │─────────────────►│ Starting │
└────────────┘                  └────┬─────┘
      ▲                              │
      │                              ▼
      │ stop()                  ┌─────────┐
      └─────────────────────────│ Running │◄──────┐
                                └────┬────┘       │
                                     │            │ validate_setup()
                             health  │            │
                             check   ▼            │
                             ┌──────────────┐     │
                             │ SetupRequired│─────┘
                             └──────┬───────┘ perform_setup()
                                    │
                                    ▼
                              ┌────────────┐
                              │SettingUp   │
                              └────────────┘
```

### Service Setup Flow

Services implementing `ServiceSetup` trait can perform one-time initialization:

1. **Check Setup Status**: `is_setup_complete()` determines if setup is needed
2. **Perform Setup**: `perform_setup()` executes idempotent setup operations  
3. **Validate Setup**: `validate_setup()` ensures setup completed correctly
4. **State Transitions**: SetupRequired → SettingUp → SetupComplete → Running

## Configuration Flow

### Standard Harness Flow
1. **YAML Config** → harness-config parsing
2. **Resolution Context** → Variable substitution, service references
3. **ServiceConfig** → service-orchestration types
4. **Command Building** → command-executor Command type
5. **Execution** → Process/Container/SSH execution

### Specialized Daemon Flow (graph-test-daemon)
1. **YAML Stack Config** → GraphStackConfig parsing
2. **Service Type Mapping** → service_type links to Service implementations
3. **Target Extraction** → Parameters extracted from ServiceTarget
4. **Service Registration** → Services registered in BaseDaemon ServiceStack
5. **Action Binding** → Service actions exposed through daemon API

Example graph-test-daemon YAML:
```yaml
name: graph-test-stack
services:
  postgres:
    service_type: postgres  # → PostgresService with database actions
    target:
      type: docker      # Managed Docker container
      image: postgres:14
      env: { POSTGRES_DB: "graph-node" }
      ports: [5432]
  anvil:
    service_type: anvil     # → AnvilService with blockchain actions  
    target:
      type: process     # Local process
      binary: anvil
      args: ["--port", "8545"]
  graph-node:
    service_type: graph-node
    target:
      type: docker-attach  # Attach to existing container
      container: graph-node-container
      env: {}
    dependencies:
      - service: postgres
      - service: ipfs
      - task: deploy-contracts  # Depends on deployment task

tasks:
  deploy-contracts:
    task_type: graph-contracts  # → GraphContractsTask
    action:
      type: DeployAll
      ethereum_url: "${anvil.url}"
      working_dir: "./contracts"
    dependencies:
      - service: anvil  # Needs blockchain running

dependencies:
  # Service dependencies use object syntax
  - service: service-name
  # Task dependencies use object syntax  
  - task: task-name
```

### Deployment Tasks

Tasks are one-time operations that run to completion:

```rust
#[async_trait]
pub trait DeploymentTask {
    type Action: DeserializeOwned + JsonSchema;
    type Event: Serialize;
    
    fn task_type() -> &'static str;
    async fn is_completed(&self) -> Result<bool>;
    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
    
    // Optional: process command events into domain events
    fn process_event(&self, event: &ProcessEvent) -> Option<Self::Event>;
}
```

### Runtime Validation

The BaseDaemon performs runtime validation when loading YAML configurations:

1. **Service Type Validation**: All `service_type` values must have registered implementations
2. **Task Type Validation**: All `task_type` values must have registered implementations  
3. **Dependency Validation**: All dependencies must reference existing services or tasks

```rust
// In daemon builder
let mut builder = BaseDaemon::builder();

// Register services
builder.service_stack_mut()
    .register("postgres-1", PostgresService::new())?;

// Register tasks  
builder.task_stack_mut()
    .register("contracts", GraphContractsTask::new())?;

// Build with validation
let daemon = builder
    .with_config(yaml_config)  // Validates against registered types
    .build()
    .await?;
```

### Service Target Variants

Services can be managed (we control lifecycle) or attached (connect to existing):

- **process**: Launch and manage local process
- **docker**: Create and manage Docker container
- **docker-attach**: Connect to existing Docker container
- **process-attach**: Connect to existing local process  
- **remote**: Execute on remote host via SSH

## Current Implementation Details

### ProcessExecutor Flow
1. Receives `ServiceConfig` with `ServiceTarget::Process`
2. Builds `Command` using command-executor
3. Launches via `Executor<LocalLauncher>` with `ManagedProcess` target
4. Stores process handle in `Arc<Mutex<HashMap>>`
5. Returns `RunningService` with PID and network info

### DockerExecutor Flow
1. Receives `ServiceConfig` with `ServiceTarget::Docker`
2. Uses Docker CLI directly (not command-executor layers)
3. Manages container lifecycle with docker commands
4. Streams logs via `docker logs --follow`
5. Tracks container state and health

### Health Monitoring
- `HealthChecker` runs periodic health checks
- Supports command-based and HTTP health checks
- Configurable intervals, timeouts, and retries
- Updates service status based on health

### Event Streaming Architecture
```rust
ProcessExecutor ──► command-executor ──► ProcessEventStream
                                              │
                                              ▼
                                        SharedEventStream
                                              │
                         ┌────────────────────┼────────────────────┐
                         ▼                    ▼                    ▼
                    Health Monitor      Service Manager      CLI Client
```

## Benefits of This Architecture

1. **Separation of Concerns**: 
   - Execution mechanism (command-executor) separate from lifecycle (service-orchestration)
   - Configuration (harness-config) separate from runtime management

2. **Composability**:
   - Layers can be stacked for complex execution scenarios
   - Executors can be mixed (local + Docker + SSH)
   - Services can depend on each other

3. **Type Safety**:
   - Strongly typed service configurations
   - Compile-time verification of target types
   - Type-safe handle operations

4. **Flexibility**:
   - Easy to add new executors
   - Easy to add new layers
   - Protocol allows extension without breaking changes

5. **Observability**:
   - Real-time event streaming
   - Structured logging with tracing
   - Persistent state for debugging

## Future Enhancements

1. **Missing Executors**:
   - `SshExecutor`: Direct SSH service management
   - `SystemdExecutor`: Production service integration
   - `KubernetesExecutor`: K8s pod management

2. **Advanced Features**:
   - Service mesh integration
   - Distributed tracing
   - Metrics collection
   - Web UI dashboard

3. **Improvements**:
   - Better error recovery
   - Service migration between hosts
   - Rolling updates
   - Canary deployments