# ADR-006: Container Path Management and Volume Mounts

## Status

Accepted

## Context

The local-network-harness manages multiple directory paths that need to be accessible both from the host system and within Docker containers. This includes:

1. **local-network directory**: Contains docker-compose.yaml and service configurations
2. **docker-test-env directory**: Contains the DinD container configuration
3. **project root**: The workspace being tested (mounted as /workspace)
4. **log directory**: Where test logs are stored
5. **test activity directory**: Temporary files and test artifacts

The challenge is maintaining consistent path references across different execution contexts:
- Host machine paths (development environment)
- DinD container paths (test execution environment)
- Service container paths (within the DinD environment)

## Decision

### Path Strategy

1. **Explicit Path Requirements**
   - The `local-network` path must be explicitly provided via CLI parameter
   - No assumptions or defaults for critical paths
   - Validate paths exist before container operations

2. **Canonical Path Resolution**
   - All paths are canonicalized (made absolute) during initialization
   - Relative paths are resolved from the project root
   - Symlinks are resolved to their actual locations

3. **Volume Mount Conventions**
   ```
   Host Path                    → Container Path
   ----------------------------------------
   {project_root}              → /workspace
   {local-network}             → /local-network
   {docker-test-env}           → (used for compose operations)
   {log_dir}                   → /logs
   ```

4. **Path Validation**
   - Check existence before mounting
   - Verify expected files (e.g., docker-compose.yaml)
   - Clear error messages for missing paths

### Implementation

```rust
// Path configuration in ContainerConfig
pub struct ContainerConfig {
    pub docker_test_env_path: PathBuf,  // Host path to docker-test-env
    pub local_network_path: PathBuf,    // Host path to local-network
    pub project_root: PathBuf,          // Host path to project root
    pub log_dir: PathBuf,               // Host path to log directory
    // ... other fields
}

// Volume mounts in DinD container
let mounts = vec![
    format!("{}:/workspace", project_root),
    format!("{}:/local-network", local_network_path),
    format!("{}:/logs", log_dir),
];
```

### Environment Variables

The DinD container receives path information through environment variables:
- `LOCAL_NETWORK_PATH`: Set to the host path for compose operations
- Working directories are set explicitly for each exec operation

## Consequences

### Positive

- **Explicit is better than implicit**: No hidden path assumptions
- **Portability**: Works across different host environments
- **Debugging**: Clear path validation errors help troubleshooting
- **Flexibility**: Users can organize their projects however they prefer

### Negative

- **More configuration**: Users must specify paths explicitly
- **No convenience defaults**: Can't just run without parameters

### Notes

- The `/workspace` mount point is hardcoded as it represents the standard workspace location
- The `/local-network` path was previously a submodule but is now a flexible mount
- Log paths can be customized or use the default test-activity location