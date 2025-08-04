# Service Setup and Deployment Tasks

## Overview

The harness provides two key abstractions for managing service initialization and one-time operations:

1. **ServiceSetup**: For services that require one-time initialization before becoming operational
2. **DeploymentTask**: For standalone operations that run to completion (e.g., contract deployment)

## Service Setup

### The ServiceSetup Trait

Services that need initialization implement the `ServiceSetup` trait:

```rust
#[async_trait]
pub trait ServiceSetup: Service {
    /// Check if the service setup has already been completed
    async fn is_setup_complete(&self) -> Result<bool>;
    
    /// Perform the one-time setup operations
    async fn perform_setup(&self) -> Result<()>;
    
    /// Validate that setup completed successfully (optional)
    async fn validate_setup(&self) -> Result<()> {
        Ok(())
    }
}
```

### Service States

The `ServiceState` enum tracks service lifecycle including setup:

```rust
pub enum ServiceState {
    NotStarted,      // Service has not been started
    Starting,        // Service is starting up
    Running,         // Service is running normally
    SetupRequired,   // Service requires setup before it can be operational
    SettingUp,       // Service is currently performing setup
    SetupComplete,   // Service setup has completed successfully
    Failed(String),  // Service has failed
}
```

### Example: PostgreSQL with Database Creation

```rust
pub struct PostgresService {
    db_name: String,
    port: u16,
}

#[async_trait]
impl ServiceSetup for PostgresService {
    async fn is_setup_complete(&self) -> Result<bool> {
        // Check if database exists
        let exists = self.check_database_exists(&self.db_name).await?;
        Ok(exists)
    }
    
    async fn perform_setup(&self) -> Result<()> {
        // Create database if it doesn't exist
        if !self.is_setup_complete().await? {
            self.create_database(&self.db_name).await?;
        }
        Ok(())
    }
    
    async fn validate_setup(&self) -> Result<()> {
        // Verify we can connect to the database
        self.test_connection(&self.db_name).await?;
        Ok(())
    }
}
```

## Deployment Tasks

### The DeploymentTask Trait

Deployment tasks are one-time operations that run to completion:

```rust
#[async_trait]
pub trait DeploymentTask: Send + Sync + 'static {
    /// Input type for task actions
    type Action: DeserializeOwned + Send + JsonSchema;
    
    /// Event type emitted during execution
    type Event: Serialize + Send;
    
    /// Task type identifier for YAML configuration
    fn task_type() -> &'static str where Self: Sized;
    
    /// Get the task name
    fn name(&self) -> &str;
    
    /// Get the task description
    fn description(&self) -> &str;
    
    /// Check if the task has already been completed
    async fn is_completed(&self) -> Result<bool>;
    
    /// Execute the task, returning a stream of events
    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>>;
    
    /// Process command executor events into domain-specific events (optional)
    fn process_event(&self, event: &ProcessEvent) -> Option<Self::Event> {
        None
    }
}
```

### Example: Smart Contract Deployment

```rust
pub struct GraphContractsTask {
    ethereum_url: String,
    working_dir: String,
    executor: Executor<LocalLauncher>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub enum GraphContractsAction {
    DeployAll {
        ethereum_url: String,
        working_dir: String,
    },
    DeployContract {
        name: String,
    },
}

#[derive(Serialize, Deserialize)]
pub enum GraphContractsEvent {
    DeploymentStarted,
    ContractCompiling { name: String },
    ContractDeployed { name: String, address: String },
    DeploymentCompleted { addresses: HashMap<String, String> },
}

#[async_trait]
impl DeploymentTask for GraphContractsTask {
    type Action = GraphContractsAction;
    type Event = GraphContractsEvent;
    
    fn task_type() -> &'static str {
        "graph-contracts"
    }
    
    async fn is_completed(&self) -> Result<bool> {
        // Check if contracts are already deployed
        let artifacts = Path::new(&self.working_dir).join("artifacts");
        Ok(artifacts.exists())
    }
    
    async fn execute(&self, action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::bounded(100);
        
        match action {
            GraphContractsAction::DeployAll { .. } => {
                // Build hardhat command
                let mut cmd = Command::new("npx");
                cmd.args(["hardhat", "deploy", "--network", "localhost"]);
                cmd.env("ETHEREUM_URL", &self.ethereum_url);
                cmd.current_dir(&self.working_dir);
                
                // Launch and stream events
                let (mut event_stream, _handle) = self.executor
                    .launch(&Target::Command, cmd)
                    .await?;
                
                // Process command events into domain events
                while let Some(event) = event_stream.next().await {
                    if let Some(translated) = self.process_event(&event) {
                        tx.send(translated).await?;
                    }
                }
            }
        }
        
        Ok(rx)
    }
    
    fn process_event(&self, event: &ProcessEvent) -> Option<Self::Event> {
        match &event.event_type {
            ProcessEventType::Stdout => {
                let line = &event.data;
                
                if line.contains("Compiling") {
                    let name = extract_contract_name(line);
                    Some(GraphContractsEvent::ContractCompiling { name })
                } else if line.contains("deployed at") {
                    let (name, address) = extract_deployment(line);
                    Some(GraphContractsEvent::ContractDeployed { name, address })
                } else {
                    None
                }
            }
            _ => None
        }
    }
}
```

## YAML Configuration

### Services with Dependencies on Tasks

```yaml
name: graph-stack
services:
  postgres:
    service_type: postgres
    target:
      type: docker
      image: postgres:14
      env:
        POSTGRES_DB: graph-node
        POSTGRES_USER: graph
        POSTGRES_PASSWORD: password
    
  graph-node:
    service_type: graph-node
    target:
      type: docker
      image: graphprotocol/graph-node
    dependencies:
      - service: postgres
      - service: ipfs
      - task: deploy-contracts  # Wait for contracts before starting

tasks:
  deploy-contracts:
    task_type: graph-contracts
    action:
      type: DeployAll
      ethereum_url: "http://localhost:8545"
      working_dir: "./contracts"
    dependencies:
      - service: anvil  # Need blockchain running first
```

### Managed vs Attached Services

```yaml
services:
  # Managed service - we create and control
  postgres:
    service_type: postgres
    target:
      type: docker
      image: postgres:14
      
  # Attached service - connect to existing
  existing-db:
    service_type: postgres
    target:
      type: docker-attach
      container: my-postgres-container
      
  # Attach to local process
  existing-app:
    service_type: app
    target:
      type: process-attach
      process_name: my-app
      # or use pid: 12345
```

## Runtime Validation

The BaseDaemon validates YAML configurations at load time:

```rust
// Build daemon with validation
let mut builder = BaseDaemon::builder();

// Register all service types
builder.service_stack_mut()
    .register("pg1", PostgresService::new())?
    .register("gn1", GraphNodeService::new())?;

// Register all task types  
builder.task_stack_mut()
    .register("contracts", GraphContractsTask::new())?;

// Load and validate config
let daemon = builder
    .with_config(yaml_config)  // Validates all references
    .build()
    .await?;
```

Validation ensures:
- All `service_type` values have registered implementations
- All `task_type` values have registered implementations
- All dependencies reference existing services or tasks
- No circular dependencies exist

## Implementation Guide

### Adding a New Service with Setup

1. Implement the `Service` trait for basic functionality
2. Implement `ServiceSetup` for initialization logic
3. Register the service type in your daemon
4. Add the service to your YAML configuration

### Adding a New Deployment Task

1. Define action and event types
2. Implement the `DeploymentTask` trait
3. Optionally implement `process_event` for better UX
4. Register the task type in your daemon
5. Add the task to your YAML configuration

### Dependency Resolution

The system uses topological sorting to ensure proper startup order:

1. Tasks run before services that depend on them
2. Services start in dependency order
3. Circular dependencies are detected and reported
4. Failed dependencies prevent dependent services from starting

## Best Practices

1. **Idempotent Setup**: `perform_setup()` should be safe to call multiple times
2. **Setup Validation**: Use `validate_setup()` to ensure setup succeeded
3. **Event Translation**: Convert process output to meaningful domain events
4. **Error Handling**: Provide clear error messages for setup failures
5. **State Persistence**: Consider storing setup state for crash recovery
6. **Timeout Handling**: Add timeouts for long-running setup operations
7. **Progress Reporting**: Emit events during long operations
8. **Dependency Clarity**: Document why each dependency is needed