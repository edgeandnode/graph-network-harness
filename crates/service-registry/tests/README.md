# Service Registry Integration Tests

This directory contains comprehensive integration tests for the service registry, designed to test real harness deployments against various node execution variants.

## Overview

The integration test framework validates:

- **Service Registry Core**: Registration, state management, persistence
- **Event System**: Real-time notifications and subscriptions  
- **Node Variants**: Local process, Docker, systemd, SSH execution
- **Multi-Node Coordination**: Distributed service orchestration
- **Package Management**: Deployment and lifecycle management
- **Error Handling**: Recovery and failure scenarios

## Test Organization

### Core Test Files

- **`integration_harness.rs`** - Full deployment and lifecycle tests
- **`node_variants.rs`** - Tests for different execution variants
- **`websocket_integration.rs`** - WebSocket API and event tests

### Test Infrastructure

- **`common/`** - Shared test utilities and helpers
  - `mod.rs` - Common test environment and traits
  - `test_services.rs` - Service definitions and builders
  - `websocket_client.rs` - WebSocket test client (stub)
  - `test_harness.rs` - Test orchestration utilities

### Test Runner

- **`run-integration-tests.sh`** - Automated test runner with environment detection

## Running Tests

### Prerequisites

The tests automatically detect available capabilities:
- **Docker** - For container-based service tests
- **SSH** - For remote execution tests  
- **Systemd** - For service management tests (limited in containers)

### Basic Integration Tests

Run tests with no external dependencies:

```bash
cargo test --features integration-tests
```

### Docker Tests

Run tests requiring Docker:

```bash
cargo test --features integration-tests,docker-tests
```

### SSH Tests  

Run tests requiring SSH setup:

```bash
cargo test --features integration-tests,ssh-tests
```

### All Tests

Run the comprehensive test suite:

```bash
./tests/run-integration-tests.sh
```

### Individual Test Categories

```bash
# Service lifecycle tests
cargo test --features integration-tests integration_harness

# Node variant tests  
cargo test --features integration-tests node_variants

# Event system tests
cargo test --features integration-tests websocket_integration
```

## Test Scenarios

### Node Execution Variants

#### Local Process Execution
- **Test**: `test_local_process_variant`
- **Validates**: Direct command execution on host
- **Requirements**: None

#### Docker Container Execution  
- **Test**: `test_docker_container_variant`
- **Validates**: Service execution in Docker containers
- **Requirements**: Docker daemon running

#### Systemd Service Execution
- **Test**: `test_systemd_service_variant`  
- **Validates**: Systemd service management
- **Requirements**: Systemd (limited in test containers)

#### Systemd Portable Execution
- **Test**: `test_systemd_portable_variant`
- **Validates**: Portable service deployment
- **Requirements**: Systemd with portable support

#### Remote SSH Execution
- **Test**: `test_remote_ssh_variant`
- **Validates**: SSH-based remote execution  
- **Requirements**: SSH configured for localhost

### Multi-Node Scenarios

#### Coordinator-Worker Pattern
- **Test**: `test_multi_node_coordination`
- **Validates**: Multi-node service deployment
- **Pattern**: 1 coordinator + 2 worker nodes

#### Service Dependency Graph
- **Test**: `test_service_dependency_graph`
- **Validates**: Complex service dependencies
- **Pattern**: Database → Backend → Frontend

### Event System

#### Real-time Notifications
- **Test**: `test_registry_websocket_style_events`
- **Validates**: Event generation and filtering
- **Events**: Registration, state changes, deregistration

#### Subscription Management  
- **Test**: `test_registry_subscription_management`
- **Validates**: Subscribe/unsubscribe functionality
- **Features**: Selective event filtering

### Error Handling

#### Recovery Scenarios
- **Test**: `test_error_handling_recovery`
- **Validates**: Error conditions and recovery
- **Scenarios**: Invalid transitions, duplicates, missing services

### Package Management

#### Deployment Flow
- **Test**: `test_package_deployment_flow` 
- **Validates**: Package creation and validation
- **Features**: Manifest validation, path sanitization

#### Persistence
- **Test**: `test_persistence_across_restarts`
- **Validates**: Registry state persistence
- **Features**: Save/load registry data

## Test Architecture

### Test Harness

The `TestHarness` provides orchestration utilities:

```rust
let harness = TestHarness::new().await?;
let service = create_echo_service()?;
let deployment = harness.deploy_service(service).await?;

// Execute and track service lifecycle  
harness.execute_and_track(&deployment.name, cmd, &target).await?;

// Wait for expected state
harness.wait_for_service_state(&deployment.name, ServiceState::Running, timeout).await?;
```

### Multi-Node Environment

The `MultiNodeTestEnvironment` simulates distributed deployments:

```rust
let env = MultiNodeTestEnvironment::new(3).await?;

// Deploy to different nodes
let coord = env.deploy_to_node(0, coordinator_service).await?;
let worker1 = env.deploy_to_node(1, worker_service).await?;
let worker2 = env.deploy_to_node(2, worker_service).await?;

// Verify coordination
let all_services = env.list_all_services().await?;
```

### Service Builders

Pre-configured service definitions for testing:

```rust
// Simple services
let echo_service = create_echo_service()?;
let web_service = create_web_service()?;

// Infrastructure services  
let docker_service = create_docker_service()?;
let systemd_service = create_systemd_service()?;
let remote_service = create_remote_service()?;

// Complex deployments
let stack = ServiceDependencyGraph::web_application_stack()?;
```

## WebSocket Integration

**Status**: Placeholder implementation pending WebSocket server

The WebSocket test client provides stubs for:
- Connection management
- Request/response handling
- Event subscription
- Service operations (list, get, deploy)

Once the WebSocket server is implemented, these tests will provide full API validation.

## Future Enhancements

### Planned Additions

- **Performance Tests**: Load testing and benchmarks
- **Security Tests**: Authentication and authorization
- **Network Tests**: Fault tolerance and partition handling
- **Migration Tests**: Schema and data migration
- **WebSocket Server**: Complete API implementation

### Integration Points

- **Command Executor**: Deep integration with execution backends  
- **Graph Network Harness**: End-to-end orchestration
- **External Systems**: Database, message queues, monitoring

## Troubleshooting

### Common Issues

**Docker tests failing**: Ensure Docker daemon is running and accessible
**SSH tests failing**: Configure SSH key-based auth to localhost  
**Systemd tests failing**: Limited functionality in containers
**Persistence tests failing**: File path permissions or temp directory issues

### Debug Output

Enable debug logging:
```bash
RUST_LOG=debug cargo test --features integration-tests
```

### Test Isolation

Each test uses isolated temporary directories and registry instances to prevent interference.

## Contributing

When adding new integration tests:

1. Add test scenarios to appropriate test file
2. Update test infrastructure in `common/` if needed
3. Add feature flags for external dependencies
4. Update this README with new test descriptions
5. Ensure tests are isolated and deterministic

The integration test framework provides comprehensive validation of the service registry's functionality across different execution environments and deployment scenarios.