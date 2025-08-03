# Service Orchestration Architecture

## Overview

The service orchestration system integrates several crates to provide heterogeneous service management through a daemon architecture:

```
┌─────────────────────┐
│   harness CLI       │
│  (User Interface)   │
└──────────┬──────────┘
           │ WebSocket/TLS
           ▼
┌─────────────────────┐       ┌─────────────────────┐
│   harness daemon    │       │  service-registry   │
│  (Server Process)   │◄─────►│  (Service Disco)    │
└──────────┬──────────┘       └─────────────────────┘
           │
           ▼
┌─────────────────────┐
│service-orchestration│
│  (ServiceManager)   │
└──────────┬──────────┘
           │
    ┌──────┴──────┬──────────┬────────────┐
    ▼             ▼          ▼            ▼
┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐
│Process │  │Docker  │  │  SSH   │  │Package │
│Executor│  │Executor│  │Executor│  │Deployer│
└────┬───┘  └────┬───┘  └────┬───┘  └────────┘
     │           │           │
     ▼           ▼           ▼
┌─────────────────────────────┐
│    command-executor         │
│  (Layered Execution)        │
└─────────────────────────────┘
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

### 3. service-orchestration (`crates/service-orchestration/`)
- **ServiceManager**: Central orchestrator for service lifecycle
- **Service Executors**: Different backends for service execution
  - `ProcessExecutor`: Local process management
  - `DockerExecutor`: Docker container management
  - `SshExecutor`: Remote SSH execution
  - `PackageDeployer`: Deploy packages to remote targets
- **Health Monitoring**: Service health checks and status tracking
- **Event Streaming**: Real-time service output and events

### 4. command-executor (`crates/command-executor/`)
- **Layered Execution**: Composable command execution layers
- **Backends**: LocalLauncher, with SSH/Docker via layers
- **Process Management**: Lifecycle control, stdin/stdout/stderr handling
- **Event Streaming**: Process events and output streaming

### 5. service-registry (`crates/service-registry/`)
- **Service Discovery**: Track running services and their endpoints
- **Network Topology**: Manage service network relationships
- **Persistence**: Store service state across daemon restarts

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
   impl ServiceExecutor for ProcessExecutor {
       async fn start(&self, config: ServiceConfig) -> Result<RunningService> {
           // Build command using command-executor
           let mut cmd = Command::new(binary);
           cmd.args(args).envs(env);
           
           // Launch via command-executor
           let (event_stream, handle) = self.executor.launch(&target, cmd).await?;
           
           // Track process and return RunningService
           Ok(RunningService { ... })
       }
   }
   ```

### Key Design Patterns

1. **Layered Architecture**: Each layer has clear responsibilities
2. **Protocol-based Communication**: CLI and daemon communicate via well-defined protocol
3. **Executor Pattern**: Different service types handled by specialized executors
4. **Event Streaming**: Real-time service output via async streams
5. **Runtime Agnostic**: Uses async-trait and runtime-agnostic libraries

### Service Lifecycle States

```
┌─────────┐     start()      ┌──────────┐
│ Stopped │─────────────────►│ Starting │
└─────────┘                  └────┬─────┘
     ▲                            │
     │                            ▼
     │ stop()                ┌─────────┐
     └───────────────────────│ Running │
                             └────┬────┘
                                  │ health check
                                  ▼
                             ┌───────────┐
                             │ Unhealthy │
                             └───────────┘
```

## Configuration Flow

1. **YAML Config** → harness-config parsing
2. **Resolution Context** → Variable substitution, service references
3. **ServiceConfig** → service-orchestration types
4. **Command Building** → command-executor Command type
5. **Execution** → Process/Container/SSH execution

## Benefits of This Architecture

1. **Separation of Concerns**: Each crate has a focused responsibility
2. **Daemon Stability**: Long-running services managed by persistent daemon
3. **Flexible Execution**: Support for local, Docker, SSH, and package deployment
4. **Real-time Monitoring**: Event streaming for service output
5. **Extensibility**: Easy to add new executors or layers
6. **Runtime Agnostic**: Works with any async runtime

## Future Enhancements

1. **Systemd Integration**: Add SystemdExecutor for production services
2. **Multi-node Orchestration**: Coordinate services across multiple hosts
3. **Service Mesh**: Advanced networking and service discovery
4. **Metrics Collection**: Prometheus/OpenTelemetry integration
5. **Web UI**: Browser-based management interface