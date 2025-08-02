# service-orchestration

Heterogeneous service orchestration library implementing ADR-007. This crate provides foundational service lifecycle management, event streaming, and multi-backend execution support as a library that other components of the graph-network-harness consume.

## Overview

This library crate implements the core service orchestration layer with the following capabilities:

- **Service Lifecycle Management**: Start and stop services across different execution environments
- **Multi-Backend Execution**: Local processes, Docker containers, and remote SSH execution
- **Event Streaming**: Real-time service event monitoring via `stream_events` API
- **Health Monitoring**: Configurable health checks for service status tracking
- **Service Registry Integration**: Automatic service registration and discovery
- **Package Deployment**: Deploy service packages to remote targets (WireGuard and LAN)

This is a library crate consumed by other crates that pass `ServiceConfig` structs to orchestrate services. For detailed configuration options, see the `config` module documentation.

## Core Concepts

### ServiceConfig

The central configuration struct that defines how services should be deployed and managed. Services are defined using `ServiceConfig` which specifies the execution target, dependencies, and health checks. See the `config` module for complete configuration options and service target types.

### ServiceManager

The central orchestrator that manages service lifecycles across different execution environments:

```rust
use service_orchestration::{ServiceManager, ServiceConfig};

let manager = ServiceManager::new().await?;

// Start a service
let running_service = manager.start_service("my-service", config).await?;

// Check service status
let status = manager.get_service_status("my-service").await?;

// Stop a service
manager.stop_service("my-service").await?;

// List all active services
let services = manager.list_services().await?;
```

### ServiceExecutor Trait

The library provides different executors for various service backends. Each executor implements the `ServiceExecutor` trait:

```rust
use service_orchestration::{ServiceExecutor, ProcessExecutor};

let executor = ProcessExecutor::new();
let running_service = executor.start(config).await?;
let event_stream = executor.stream_events(&running_service).await?;
```

## Service Execution Backends

The library supports multiple execution backends through the `ServiceTarget` enum defined in the `config` module:

- **Local Process**: Execute services as local processes with PID tracking
- **Docker Container**: Manage Docker containers with full lifecycle support  
- **Remote LAN**: Deploy to remote hosts via SSH connections
- **WireGuard Package**: Deploy pre-built packages to WireGuard peers

Each target type has specific configuration requirements. See the `config::ServiceTarget` documentation for complete details on configuring each backend type.

## Health Monitoring

The library provides configurable health checks through the `HealthCheck` configuration struct. Services can be monitored for health status using command-based health checks with configurable intervals, retries, and timeouts.

Health monitoring is handled by the `HealthMonitor` and integrated into the service lifecycle. See the `config::HealthCheck` and `health` module documentation for complete health check configuration options.

## Service Dependencies

Services can declare dependencies on other services through the `dependencies` field in `ServiceConfig`. The orchestrator handles dependency resolution and ensures services start in the correct order.

## State Management

The `ServiceManager` provides persistent state management for service configurations and runtime information. Service state is automatically persisted and can survive orchestrator restarts. The manager integrates with the service registry for service discovery and state tracking across the system.

## Package Deployment

The library includes a package deployment system for remote service management. Service packages can be deployed to remote targets with automatic installation and lifecycle management. See the `package` module for package format specifications and deployment APIs.

## Event Streaming

### Service Event Streaming

The library provides real-time event streaming from running services through the `stream_events` API:

```rust
use service_orchestration::ServiceExecutor;
use command_executor::event::{ProcessEvent, ProcessEventType};
use futures::StreamExt;

// Start a service
let service = executor.start(config).await?;

// Stream events from the service
let mut event_stream = executor.stream_events(&service).await?;

while let Some(event) = event_stream.next().await {
    match event.event_type {
        ProcessEventType::Stdout => {
            println!("[{}] {}", service.name, event.data);
        }
        ProcessEventType::Stderr => {
            eprintln!("[{}] ERROR: {}", service.name, event.data);
        }
        ProcessEventType::Exit => {
            println!("[{}] Process exited with code: {}", service.name, event.data);
            break;
        }
        _ => {} // Handle other event types as needed
    }
}
```

The `stream_events` API provides real-time access to:
- **Stdout/Stderr output**: Process logs and error messages
- **Exit events**: Process termination with exit codes  
- **System events**: Process lifecycle changes

All service executors implement the `stream_events` method, providing consistent event streaming across different execution backends.

## Integration with Graph Network Harness

This library crate is designed to be consumed by other components of the graph-network-harness system. It implements the heterogeneous service orchestration specified in ADR-007, providing:

- **Runtime-Agnostic Design**: Works with any async runtime (Tokio, async-std, smol)
- **Composable Architecture**: Integrates with the service registry and network management layers
- **Unified Interface**: Consistent APIs across different execution backends
- **Event-Driven**: Real-time monitoring and event streaming for service lifecycle management

The library serves as the foundation for orchestrating complex distributed systems with services running across local processes, containers, and remote hosts.

## API Reference

The library exposes the following main components:

- **`ServiceManager`**: Central orchestrator for service lifecycle management
- **`ServiceConfig`**: Configuration struct for defining services (see `config` module)
- **`ServiceExecutor`**: Trait implemented by execution backend handlers
- **`RunningService`**: Runtime information about active service instances
- **`HealthChecker`**: Health monitoring and status checking
- **`PackageDeployer`**: Remote package deployment functionality

### Usage Example

```rust
use service_orchestration::{ServiceManager, ServiceConfig, ServiceTarget};
use std::collections::HashMap;

// Create service manager
let manager = ServiceManager::new().await?;

// Define a service configuration
let config = ServiceConfig {
    name: "my-service".to_string(),
    target: ServiceTarget::Process {
        binary: "my-app".to_string(),
        args: vec!["--port".to_string(), "8080".to_string()],
        env: HashMap::new(),
        working_dir: None,
    },
    dependencies: vec![],
    health_check: None,
};

// Start the service
let running_service = manager.start_service("my-service", config).await?;

// Check status
let status = manager.get_service_status("my-service").await?;
```

## Testing

The library includes comprehensive tests for all service execution backends:

```bash
# Run all tests
cargo test -p service-orchestration

# Run with Docker backend tests
cargo test -p service-orchestration --features docker-tests

# Run with SSH tests
cargo test -p service-orchestration --features ssh-tests
```

The test suite validates service lifecycle management, event streaming, health monitoring, and cross-backend compatibility using the runtime-agnostic `smol_potat::test` framework.

## License

Licensed under MIT or Apache-2.0, at your option.