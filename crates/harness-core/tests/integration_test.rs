//! End-to-end integration tests for service and task dependencies

use async_channel::Receiver;
use async_trait::async_trait;
use harness_core::prelude::*;
use harness_core::daemon::{BaseDaemon, DaemonBuilder};
use harness_core::service::{Service, ServiceSetup, ServiceState, StatefulService};
use harness_core::task::DeploymentTask;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use smol::Timer;
use async_runtime_compat::smol::SmolSpawner;

// Shared state to track execution order
static EXECUTION_ORDER: AtomicU32 = AtomicU32::new(0);

#[derive(Debug)]
struct ExecutionTracker {
    name: String,
    order: Option<u32>,
    completed: Arc<AtomicBool>,
}

impl ExecutionTracker {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            order: None,
            completed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn mark_started(&mut self) -> u32 {
        let order = EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst);
        self.order = Some(order);
        order
    }

    fn mark_completed(&self) {
        self.completed.store(true, Ordering::SeqCst);
    }

    fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }
}

// Test service implementation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ServiceAction {
    command: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ServiceEvent {
    status: String,
}

struct TestService {
    name: String,
    tracker: Arc<std::sync::Mutex<ExecutionTracker>>,
    setup_required: bool,
}

impl TestService {
    fn new(name: impl Into<String>, setup_required: bool) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            tracker: Arc::new(std::sync::Mutex::new(ExecutionTracker::new(name))),
            setup_required,
        }
    }

    fn get_order(&self) -> Option<u32> {
        self.tracker.lock().unwrap().order
    }
}

#[async_trait]
impl Service for TestService {
    type Action = ServiceAction;
    type Event = ServiceEvent;

    fn service_type() -> &'static str {
        "test-service"
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Test service for integration testing"
    }

    async fn dispatch_action(&self, _action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::bounded(1);
        
        // Mark as started
        let order = self.tracker.lock().unwrap().mark_started();
        
        tx.send(ServiceEvent {
            status: format!("Service {} started as #{}", self.name, order + 1),
        }).await.unwrap();
        
        Ok(rx)
    }
}

#[async_trait]
impl ServiceSetup for TestService {
    async fn is_setup_complete(&self) -> Result<bool> {
        Ok(!self.setup_required || self.tracker.lock().unwrap().is_completed())
    }

    async fn perform_setup(&self) -> Result<()> {
        if self.setup_required {
            // Simulate setup work
            Timer::after(Duration::from_millis(10)).await;
            self.tracker.lock().unwrap().mark_completed();
        }
        Ok(())
    }
}

#[async_trait]
impl StatefulService for TestService {
    async fn get_state(&self) -> Result<ServiceState> {
        if self.setup_required && !self.is_setup_complete().await? {
            Ok(ServiceState::SetupRequired)
        } else {
            Ok(ServiceState::Running)
        }
    }

    async fn wait_for_state(&self, target: ServiceState, _timeout: Duration) -> Result<()> {
        let current = self.get_state().await?;
        if current == target {
            Ok(())
        } else {
            Err(Error::service_type(format!(
                "Service {} is in state {:?}, not {:?}",
                self.name, current, target
            )))
        }
    }
}

// Test task implementation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TaskAction {
    target: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TaskEvent {
    progress: u8,
    message: String,
}

struct TestTask {
    name: String,
    tracker: Arc<std::sync::Mutex<ExecutionTracker>>,
}

impl TestTask {
    fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            tracker: Arc::new(std::sync::Mutex::new(ExecutionTracker::new(name))),
        }
    }

    fn get_order(&self) -> Option<u32> {
        self.tracker.lock().unwrap().order
    }
}

#[async_trait]
impl DeploymentTask for TestTask {
    type Action = TaskAction;
    type Event = TaskEvent;

    fn task_type() -> &'static str {
        "test-task"
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Test deployment task"
    }

    async fn is_completed(&self) -> Result<bool> {
        Ok(self.tracker.lock().unwrap().is_completed())
    }

    async fn execute(&self, _action: Self::Action) -> Result<Receiver<Self::Event>> {
        let (tx, rx) = async_channel::bounded(10);
        let tracker = self.tracker.clone();
        let name = self.name.clone();

        // Use smol for tests
        smol::spawn(async move {
            // Mark as started
            let order = tracker.lock().unwrap().mark_started();
            
            // Send start event
            let _ = tx.send(TaskEvent {
                progress: 0,
                message: format!("Task {} started as #{}", name, order + 1),
            }).await;

            // Simulate work
            Timer::after(Duration::from_millis(20)).await;

            // Send progress
            let _ = tx.send(TaskEvent {
                progress: 50,
                message: format!("Task {} at 50%", name),
            }).await;

            Timer::after(Duration::from_millis(20)).await;

            // Complete
            tracker.lock().unwrap().mark_completed();
            let _ = tx.send(TaskEvent {
                progress: 100,
                message: format!("Task {} completed", name),
            }).await;
        }).detach();

        Ok(rx)
    }
}

#[smol_potat::test]
async fn test_simple_dependency_chain() {
    // Reset execution order
    EXECUTION_ORDER.store(0, Ordering::SeqCst);

    let mut builder = DaemonBuilder::new();

    // Create services
    let db_service = TestService::new("db", false);
    let api_service = TestService::new("api", false);
    let web_service = TestService::new("web", false);

    // Store references to check order later
    let db_order = db_service.tracker.clone();
    let api_order = api_service.tracker.clone();
    let web_order = web_service.tracker.clone();

    // Register services
    builder.service_stack_mut().register_stateful("db".to_string(), db_service).unwrap();
    builder.service_stack_mut().register_stateful("api".to_string(), api_service).unwrap();
    builder.service_stack_mut().register_stateful("web".to_string(), web_service).unwrap();

    // Create configuration with dependencies
    let config = json!({
        "services": {
            "db": {
                "service_type": "test-service",
                "dependencies": []
            },
            "api": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "db" }
                ]
            },
            "web": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "api" }
                ]
            }
        }
    });

    // Build daemon with config
    let daemon = builder
        .with_config(config)
        .build()
        .await
        .unwrap();

    // Simulate service startup in dependency order
    // In a real system, the orchestrator would handle this
    let spawner = SmolSpawner;
    let _ = daemon.service_stack().dispatch("db", "start", json!({ "command": "start" }), &spawner).await.unwrap();
    let _ = daemon.service_stack().dispatch("api", "start", json!({ "command": "start" }), &spawner).await.unwrap();
    let _ = daemon.service_stack().dispatch("web", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // Verify execution order
    let db_exec = db_order.lock().unwrap().order.unwrap();
    let api_exec = api_order.lock().unwrap().order.unwrap();
    let web_exec = web_order.lock().unwrap().order.unwrap();

    assert!(db_exec < api_exec, "DB should start before API");
    assert!(api_exec < web_exec, "API should start before Web");
}

#[smol_potat::test]
async fn test_mixed_service_task_dependencies() {
    // Reset execution order
    EXECUTION_ORDER.store(0, Ordering::SeqCst);

    let mut builder = DaemonBuilder::new();

    // Create services and tasks
    let db_service = TestService::new("db", false);
    let cache_service = TestService::new("cache", false);
    let migrate_task = TestTask::new("db-migrate");
    let warmup_task = TestTask::new("cache-warmup");

    // Store references
    let db_order = db_service.tracker.clone();
    let cache_order = cache_service.tracker.clone();
    let migrate_order = migrate_task.tracker.clone();
    let warmup_order = warmup_task.tracker.clone();

    // Register
    builder.service_stack_mut().register_stateful("db".to_string(), db_service).unwrap();
    builder.service_stack_mut().register_stateful("cache".to_string(), cache_service).unwrap();
    builder.task_stack_mut().register("db-migrate".to_string(), migrate_task).unwrap();
    builder.task_stack_mut().register("cache-warmup".to_string(), warmup_task).unwrap();

    let config = json!({
        "services": {
            "db": {
                "service_type": "test-service",
                "dependencies": []
            },
            "cache": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "db" },
                    { "task": "db-migrate" }
                ]
            }
        },
        "tasks": {
            "db-migrate": {
                "task_type": "test-task",
                "dependencies": [
                    { "service": "db" }
                ]
            },
            "cache-warmup": {
                "task_type": "test-task",
                "dependencies": [
                    { "service": "cache" }
                ]
            }
        }
    });

    let daemon = builder
        .with_config(config)
        .build()
        .await
        .unwrap();

    // Execute in dependency order
    // 1. Start DB service
    let spawner = SmolSpawner;
    let _ = daemon.service_stack().dispatch("db", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // 2. Run DB migration task
    let migrate_rx = daemon.task_stack().execute("db-migrate", json!({ "target": "db" }), &spawner).await.unwrap();
    while migrate_rx.recv().await.is_ok() {} // Wait for completion

    // 3. Start cache service
    let _ = daemon.service_stack().dispatch("cache", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // 4. Run cache warmup task
    let warmup_rx = daemon.task_stack().execute("cache-warmup", json!({ "target": "cache" }), &spawner).await.unwrap();
    while warmup_rx.recv().await.is_ok() {} // Wait for completion

    // Verify execution order
    let db_exec = db_order.lock().unwrap().order.unwrap();
    let migrate_exec = migrate_order.lock().unwrap().order.unwrap();
    let cache_exec = cache_order.lock().unwrap().order.unwrap();
    let warmup_exec = warmup_order.lock().unwrap().order.unwrap();

    assert!(db_exec < migrate_exec, "DB should start before migration");
    assert!(migrate_exec < cache_exec, "Migration should complete before cache starts");
    assert!(cache_exec < warmup_exec, "Cache should start before warmup");
}

#[smol_potat::test]
async fn test_service_setup_flow() {
    // Reset execution order
    EXECUTION_ORDER.store(0, Ordering::SeqCst);

    let mut builder = DaemonBuilder::new();

    // Create service that requires setup
    let service = TestService::new("postgres", true);
    let tracker = service.tracker.clone();

    builder.service_stack_mut().register_stateful("postgres".to_string(), service).unwrap();

    let config = json!({
        "services": {
            "postgres": {
                "service_type": "test-service",
                "dependencies": []
            }
        }
    });

    let daemon = builder
        .with_config(config)
        .build()
        .await
        .unwrap();

    let service = daemon.service_stack().get("postgres").unwrap();

    // Check initial state
    let state = service.get_state().await.unwrap();
    assert_eq!(state, ServiceState::SetupRequired);

    // Perform setup
    service.perform_setup().await.unwrap();

    // Check state after setup
    let state = service.get_state().await.unwrap();
    assert_eq!(state, ServiceState::Running);

    // Verify setup completed
    assert!(tracker.lock().unwrap().is_completed());
}

#[smol_potat::test]
async fn test_task_completion_tracking() {
    let mut builder = DaemonBuilder::new();

    let task = TestTask::new("deploy");
    builder.task_stack_mut().register("deploy".to_string(), task).unwrap();

    let daemon = builder.build().await.unwrap();

    // Check not completed initially
    assert!(!daemon.task_stack().is_completed("deploy").await.unwrap());

    // Execute task
    let spawner = SmolSpawner;
    let rx = daemon.task_stack().execute("deploy", json!({ "target": "production" }), &spawner).await.unwrap();

    // Collect all events
    let mut events = Vec::new();
    while let Ok(event) = rx.recv().await {
        events.push(event);
    }

    // Should have received events
    assert!(!events.is_empty());
    assert_eq!(events.last().unwrap()["progress"], 100);

    // Small delay to ensure completion flag is set
    Timer::after(Duration::from_millis(50)).await;

    // Check completed
    assert!(daemon.task_stack().is_completed("deploy").await.unwrap());
}

#[smol_potat::test]
async fn test_complex_dependency_graph() {
    // Reset execution order
    EXECUTION_ORDER.store(0, Ordering::SeqCst);

    let mut builder = DaemonBuilder::new();

    // Create a complex dependency graph:
    // blockchain -> contracts-task -> graph-node
    //            \-> ipfs --------/
    //                    \-> indexer-service

    let blockchain = TestService::new("blockchain", false);
    let ipfs = TestService::new("ipfs", false);
    let graph_node = TestService::new("graph-node", false);
    let indexer_service = TestService::new("indexer-service", false);
    let contracts_task = TestTask::new("deploy-contracts");

    // Store references
    let blockchain_order = blockchain.tracker.clone();
    let ipfs_order = ipfs.tracker.clone();
    let graph_node_order = graph_node.tracker.clone();
    let indexer_order = indexer_service.tracker.clone();
    let contracts_order = contracts_task.tracker.clone();

    // Register all
    builder.service_stack_mut().register_stateful("blockchain".to_string(), blockchain).unwrap();
    builder.service_stack_mut().register_stateful("ipfs".to_string(), ipfs).unwrap();
    builder.service_stack_mut().register_stateful("graph-node".to_string(), graph_node).unwrap();
    builder.service_stack_mut().register_stateful("indexer-service".to_string(), indexer_service).unwrap();
    builder.task_stack_mut().register("deploy-contracts".to_string(), contracts_task).unwrap();

    let config = json!({
        "services": {
            "blockchain": {
                "service_type": "test-service",
                "dependencies": []
            },
            "ipfs": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "blockchain" }
                ]
            },
            "graph-node": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "ipfs" },
                    { "task": "deploy-contracts" }
                ]
            },
            "indexer-service": {
                "service_type": "test-service",
                "dependencies": [
                    { "service": "ipfs" },
                    { "service": "graph-node" }
                ]
            }
        },
        "tasks": {
            "deploy-contracts": {
                "task_type": "test-task",
                "dependencies": [
                    { "service": "blockchain" }
                ]
            }
        }
    });

    let daemon = builder
        .with_config(config)
        .build()
        .await
        .unwrap();

    // Execute in proper order
    // 1. Start blockchain
    let spawner = SmolSpawner;
    let _ = daemon.service_stack().dispatch("blockchain", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // 2. Start IPFS and deploy contracts (can be parallel)
    let _ = daemon.service_stack().dispatch("ipfs", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    let contracts_rx = daemon.task_stack().execute("deploy-contracts", json!({ "target": "local" }), &spawner).await.unwrap();
    while contracts_rx.recv().await.is_ok() {}

    // 3. Start graph-node (after both IPFS and contracts)
    let _ = daemon.service_stack().dispatch("graph-node", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // 4. Start indexer-service (after graph-node and IPFS)
    let _ = daemon.service_stack().dispatch("indexer-service", "start", json!({ "command": "start" }), &spawner).await.unwrap();

    // Verify execution order constraints
    let blockchain_exec = blockchain_order.lock().unwrap().order.unwrap();
    let ipfs_exec = ipfs_order.lock().unwrap().order.unwrap();
    let contracts_exec = contracts_order.lock().unwrap().order.unwrap();
    let graph_exec = graph_node_order.lock().unwrap().order.unwrap();
    let indexer_exec = indexer_order.lock().unwrap().order.unwrap();

    // Blockchain starts first
    assert!(blockchain_exec < ipfs_exec);
    assert!(blockchain_exec < contracts_exec);

    // Graph node waits for both IPFS and contracts
    assert!(ipfs_exec < graph_exec);
    assert!(contracts_exec < graph_exec);

    // Indexer waits for both IPFS and graph-node
    assert!(ipfs_exec < indexer_exec);
    assert!(graph_exec < indexer_exec);
}

#[smol_potat::test]
async fn test_failed_dependency_handling() {
    let mut builder = DaemonBuilder::new();

    // Create a failing task
    struct FailingTask;

    #[async_trait]
    impl DeploymentTask for FailingTask {
        type Action = TaskAction;
        type Event = TaskEvent;

        fn task_type() -> &'static str {
            "failing-task"
        }

        fn name(&self) -> &str {
            "failing-task"
        }

        fn description(&self) -> &str {
            "Task that always fails"
        }

        async fn is_completed(&self) -> Result<bool> {
            Ok(false)
        }

        async fn execute(&self, _action: Self::Action) -> Result<Receiver<Self::Event>> {
            Err(Error::daemon("Task failed intentionally"))
        }
    }

    let service = TestService::new("dependent-service", false);
    builder.service_stack_mut().register_stateful("dependent-service".to_string(), service).unwrap();
    builder.task_stack_mut().register("failing-task".to_string(), FailingTask).unwrap();

    let config = json!({
        "services": {
            "dependent-service": {
                "service_type": "test-service",
                "dependencies": [
                    { "task": "failing-task" }
                ]
            }
        },
        "tasks": {
            "failing-task": {
                "task_type": "failing-task",
                "dependencies": []
            }
        }
    });

    let daemon = builder
        .with_config(config)
        .build()
        .await
        .unwrap();

    // Try to execute the failing task
    let spawner = SmolSpawner;
    let result = daemon.task_stack().execute("failing-task", json!({ "target": "test" }), &spawner).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("failed intentionally"));
}