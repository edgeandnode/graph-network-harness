# Graph Protocol Local Network Test Harness

A comprehensive integration testing framework for applications that interact with The Graph Protocol's local-network stack. Provides Docker-in-Docker (DinD) isolation for testing against graph-node, indexer services, contracts, and the full Graph Protocol infrastructure.

## Overview

The test harness provides:
- **Binary CLI**: Ready-to-use command-line interface for testing
- **Library API**: Reusable harness for programmatic integration
- **Docker-in-Docker**: Complete container isolation preventing conflicts
- **Local Network Stack**: Full Graph Protocol infrastructure (graph-node, indexer services, contracts)
- **Persistent Logging**: All logs stored in organized session directories
- **Image Synchronization**: Efficient Docker image management between host and containers

## Quick Start

### Using the Binary

```bash
# Run all integration tests
target/debug/stack-runner all --local-network ../local-network

# Start the harness for manual testing
target/debug/stack-runner harness --keep-running --local-network ../local-network

# Run tests in Docker container (recommended for isolation)
target/debug/stack-runner container --sync-images --local-network ../local-network

# Use custom local-network path
target/debug/stack-runner container --sync-images --local-network /path/to/your/local-network

# Execute commands in the test environment
target/debug/stack-runner exec --local-network ../local-network -- docker ps

# View logs from test sessions
target/debug/stack-runner logs --summary
```

### As a Library

```rust
use local_network_harness::{LocalNetworkHarness, HarnessConfig};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = HarnessConfig::default();
    config.local_network_path = PathBuf::from("../local-network");
    let mut harness = LocalNetworkHarness::new(config)?;
    
    // Start the test environment
    harness.start().await?;
    
    // Start the local network
    harness.start_local_network().await?;
    
    // Run your tests
    let mut ctx = local_network_harness::TestContext::new(&mut harness);
    ctx.deploy_subgraph("/path/to/subgraph", "test-subgraph").await?;
    
    // Cleanup happens automatically on drop
    Ok(())
}
```

### Using Docker Test Environment

For complete isolation and parallel test execution:

```bash
cd docker-test-env

# Run with image syncing (recommended for first run)
cargo run --bin stack-runner -- container --sync-images --local-network ../local-network harness

# Run without sync (faster if images already synced)
cargo run --bin stack-runner -- container --local-network ../local-network all

# Interactive shell for development
docker-compose exec integration-tests-dind sh
```

## Architecture

### Docker-in-Docker (DinD) Setup

The framework uses Docker-in-Docker to provide complete isolation:

```
Host Machine
└── Docker
    └── DinD Container (integration-tests-dind)
        ├── Docker Daemon (dockerd)
        ├── Rust/Cargo environment
        ├── Integration test binary
        └── Docker Containers (managed by harness)
            ├── chain (Anvil)
            ├── postgres
            ├── ipfs
            ├── graph-node
            └── indexer-agent
```

#### Why Docker-in-Docker?

The local-network docker-compose uses hardcoded container names (e.g., `container_name: postgres`), which prevents multiple test instances from running simultaneously. DinD solves this by:

1. **Complete Isolation**: Each test run gets its own Docker daemon
2. **No Conflicts**: Multiple test runs can execute in parallel
3. **Clean State**: Each run starts with a fresh Docker environment
4. **Portability**: Tests run identically on any machine with Docker

### Test Activity Directory

All test runs create artifacts in a dedicated test-activity directory:

```
test-activity/
├── logs/                                   # Test session logs
│   ├── 2025-01-14_17-42-33_a1b2c3d4/     # Timestamp + Session ID
│   │   ├── session.json                   # Session metadata
│   │   ├── docker-compose.log             # Compose startup logs
│   │   ├── chain.log                      # Service logs
│   │   ├── graph-node.log
│   │   ├── indexer-agent.log
│   │   └── ...
│   └── current -> 2025-01-14_17-42-33_a1b2c3d4/
├── volumes/                                # Docker volumes (future)
└── artifacts/                              # Test artifacts (future)
```

This directory is excluded from git and provides a persistent location for:
- Session logs that survive test completion
- Docker volumes for data persistence
- Test artifacts and outputs
- Debugging information

### Library API

The crate exposes a high-level API for test automation:

```rust
pub use local_network_harness::{
    LocalNetworkHarness,  // Main test harness
    HarnessConfig,        // Configuration options
    TestContext,          // Test utilities
    ContainerConfig,      // Container settings
    DindManager,          // Low-level container management
};
```

## Configuration

### HarnessConfig Options

```rust
HarnessConfig {
    // Path to docker-test-env (auto-detected by default)
    docker_test_env_path: Option<PathBuf>,
    
    // Project root directory
    project_root: PathBuf,
    
    // Custom log directory (temp by default)
    log_dir: Option<PathBuf>,
    
    // Container startup timeout
    startup_timeout: Duration,
    
    // Auto-sync Docker images from host
    auto_sync_images: bool,
    
    // Build local-network images before running
    build_images: bool,
    
    // Custom session name for organization
    session_name: Option<String>,
}
```

## Test Development

### Writing Integration Tests

```rust
use local_network_harness::{LocalNetworkHarness, HarnessConfig, TestContext};
use std::path::PathBuf;

#[tokio::test]
async fn test_subgraph_deployment() -> anyhow::Result<()> {
    // Set up the test environment
    let mut config = HarnessConfig::default();
    config.local_network_path = PathBuf::from("../local-network");
    let mut harness = LocalNetworkHarness::new(config)?;
    harness.start().await?;
    harness.start_local_network().await?;
    
    // Create test context
    let mut ctx = TestContext::new(&mut harness);
    
    // Deploy a test subgraph
    ctx.deploy_subgraph("./test-subgraph", "my-test").await?;
    
    // Verify deployment
    assert!(ctx.container_running("graph-node").await?);
    
    // Check logs if needed
    ctx.container_logs("indexer-agent", Some(50)).await?;
    
    Ok(())
}
```

### Using TestContext

The `TestContext` provides utilities for common test operations:

- `exec()` - Execute commands in the test environment
- `exec_in()` - Execute with specific working directory
- `deploy_subgraph()` - Deploy subgraphs to the local network
- `container_running()` - Check container status
- `container_logs()` - Retrieve container logs

## Session Management

### Viewing Test Sessions

```bash
# List all sessions
cargo run --bin stack-runner -- logs --summary

# View specific session logs
cargo run --bin stack-runner -- logs --session 2025-01-14_17-42-33_a1b2c3d4
```

### Session Metadata

Each session creates a `session.json` with:
- Start/end times
- Test command executed
- Service statuses and exit codes
- Failure analysis (if applicable)
- Log file locations

## Image Management

### Building Images

The harness can build local-network images:

```bash
# Build before running tests
cargo run --bin stack-runner -- harness --build --local-network ../local-network

# Or in container
cargo run --bin stack-runner -- container --sync-images --build-host --local-network ../local-network harness
```

### Syncing Images

When using DinD, sync images from host for faster startup:

```bash
# Automatic sync
cargo run --bin stack-runner -- container --sync-images --local-network ../local-network all

# Manual sync is handled automatically by the harness
```

## Troubleshooting

### Common Issues

1. **Container name conflicts**: Use the DinD environment
2. **Slow startup**: Enable image syncing with `--sync-images`
3. **Permission errors**: Check log directory permissions
4. **Build failures**: Ensure Docker daemon is running

### Debug Commands

```bash
# Check container status
docker-compose -f docker-test-env/docker-compose.yaml ps

# View DinD logs
docker-compose -f docker-test-env/docker-compose.yaml logs

# Access container shell
docker-compose -f docker-test-env/docker-compose.yaml exec integration-tests-dind sh
```

## Performance Tips

1. **First Run**: Use `--sync-images` to transfer existing images
2. **Development**: Keep container running with `--keep-running`
3. **CI/CD**: Run multiple isolated instances in parallel
4. **Caching**: Cargo and Docker caches persist in named volumes

## Advanced Features

### Custom Test Harness

```rust
// Create custom configuration
let mut config = HarnessConfig::default();
config.startup_timeout = Duration::from_secs(120);
config.session_name = Some("my-test-run".to_string());

// Use custom harness
let mut harness = LocalNetworkHarness::new(config)?;
```

### Log Analysis

All logs are stored as files for analysis:

```bash
# Search logs
grep ERROR test-activity/logs/*/indexer-agent.log

# Follow logs in real-time
tail -f test-activity/logs/current/*.log

# Analyze with standard tools
less test-activity/logs/*/session.json
```

## Development

### Prerequisites

For development, you'll need the following tools:
- Rust toolchain
- Docker
- cargo-deny (for dependency constraint checking)

Install cargo-deny:
```bash
cargo install cargo-deny
```

## Development Workflow

1. **Local Development**:
   ```bash
   cargo build --bin stack-runner
   cargo run --bin stack-runner -- harness --keep-running --local-network ../local-network
   ```

2. **Test Development**:
   ```bash
   # Run in container for isolation
   cargo run --bin stack-runner -- container --local-network ../local-network harness
   
   # Make changes and re-run
   cargo build && cargo run --bin stack-runner -- container --local-network ../local-network all
   ```

3. **CI Integration**:
   ```bash
   # Build and run all tests
   cargo run --bin stack-runner -- container --sync-images --local-network ../local-network all
   ```

## Contributing

When adding new tests:
1. Use the `LocalNetworkHarness` API for consistency
2. Ensure proper cleanup in test teardown
3. Add appropriate logging for debugging
4. Document any special requirements

## License

See the main project LICENSE file.