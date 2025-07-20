# Self-Hosted Test Orchestration Design

## Overview
Use the command-executor library to orchestrate its own integration tests, replacing shell scripts with Rust code that uses the library's features.

## Current Test Infrastructure

### Shell Scripts
- `setup-test-env.sh` - Complete test environment setup
- `run-ssh-tests.sh` - Container lifecycle management
- `generate-ssh-keys.sh` - SSH key generation
- Docker Compose files for container orchestration

### Problems with Current Approach
- Manual setup required before running tests
- Shell scripts duplicate functionality the library provides
- Tests don't validate real-world usage patterns
- Error handling is primitive
- No type safety or compile-time checks

## Proposed Architecture

### Test Harness Module
Create `tests/common/test_harness.rs` that provides:

```rust
pub struct TestHarness {
    executor: Executor<LocalLauncher>,
    container_handle: Option<ProcessHandle>,
    ssh_keys_generated: bool,
}

impl TestHarness {
    pub async fn setup() -> Result<Self> {
        // 1. Generate SSH keys using Command target
        // 2. Build Docker image using Command target
        // 3. Start container using DockerContainer target
        // 4. Wait for readiness using health checks
        // 5. Return configured harness
    }
    
    pub async fn teardown(self) -> Result<()> {
        // Stop container, cleanup resources
    }
}
```

### Integration Test Structure
```rust
#[tokio::test]
async fn test_ssh_systemd_integration() {
    let harness = TestHarness::setup().await.unwrap();
    
    // Run actual test using SSH launcher
    let ssh_launcher = SshLauncher::new(LocalLauncher, harness.ssh_config());
    let executor = Executor::new("test", ssh_launcher);
    
    // Test systemd-portable functionality
    // ...
    
    harness.teardown().await.unwrap();
}
```

### Implementation Steps

1. **SSH Key Generation**
   ```rust
   let keygen_cmd = Command::builder("ssh-keygen")
       .arg("-t").arg("ed25519")
       .arg("-f").arg("test_ed25519")
       .arg("-N").arg("")
       .build();
   executor.execute(&Target::Command, keygen_cmd).await?;
   ```

2. **Docker Container Management**
   ```rust
   let container = DockerContainer::builder("systemd-ssh-test")
       .dockerfile_path("tests/systemd-container/Dockerfile")
       .port_mapping(2223, 22)
       .volume(ssh_keys_path, "/home/testuser/.ssh/authorized_keys")
       .health_check("systemctl is-system-running")
       .build();
   
   let (events, handle) = executor.launch(&Target::DockerContainer(container), cmd).await?;
   ```

3. **Health Monitoring**
   ```rust
   // Use ManagedService to attach to container logs
   let service = ManagedService::builder("systemd-container")
       .status_command(docker_ps_cmd)
       .log_command(docker_logs_cmd)
       .build();
   
   let (log_events, _) = attacher.attach(&service, config).await?;
   ```

## Benefits

1. **Dogfooding**: Tests use the library's actual features
2. **Type Safety**: Rust's type system catches configuration errors
3. **Better Error Handling**: Proper error propagation and context
4. **Self-Contained**: No manual setup required
5. **Real-World Validation**: Tests demonstrate actual usage patterns
6. **Maintainability**: Changes to library API are immediately reflected in tests

## Migration Path

1. Start with a single test file that uses the new approach
2. Gradually migrate other integration tests
3. Keep shell scripts as fallback during transition
4. Remove shell scripts once all tests are migrated

## Example Test Flow

```rust
// Test that demonstrates nested launcher usage
#[tokio::test]
async fn test_nested_ssh_docker() {
    // Setup: Use command-executor to prepare test environment
    let harness = TestHarness::setup().await.unwrap();
    
    // Create nested launcher: SSH -> Docker
    let local = LocalLauncher;
    let ssh = SshLauncher::new(local, harness.ssh_config());
    let executor = Executor::new("nested-test", ssh);
    
    // Launch container over SSH
    let container = DockerContainer::new("alpine:latest");
    let cmd = Command::new("echo").arg("Hello from nested container");
    
    let exit_status = executor.execute(&Target::DockerContainer(container), cmd).await.unwrap();
    assert_eq!(exit_status.code, Some(0));
    
    // Teardown
    harness.teardown().await.unwrap();
}
```

## Future Enhancements

1. **Parallel Test Execution**: Use multiple containers for test isolation
2. **Test Fixtures**: Reusable test environments
3. **Performance Benchmarks**: Measure library overhead
4. **Stress Testing**: Launch many processes/containers simultaneously
5. **Network Testing**: Test WireGuard integration when implemented