# LayeredServiceExecutor

The LayeredServiceExecutor addresses the architectural limitation of the RemoteSSH executor being hard-coded to use only a single layer (SSH over LocalLauncher). This new executor provides flexible composition of execution layers for complex deployment scenarios.

## Architecture Overview

The LayeredServiceExecutor leverages the command-executor crate's `LayeredExecutor` to compose arbitrary execution contexts. This enables scenarios like:

- **Multi-hop SSH**: SSH to a jump host, then SSH to the final target
- **SSH + Docker**: SSH to a remote host and execute within a Docker container
- **Local + Docker**: Execute within a local Docker container with custom environment
- **Complex chains**: Any combination of Local, SSH, and Docker layers

## Configuration

The LayeredServiceExecutor uses the `ServiceTarget::Layered` configuration:

```yaml
services:
  complex-service:
    type: layered
    layers:
      - type: ssh
        host: jump.example.com
        user: jump-user
        port: 2222
        identity_file: ~/.ssh/jump_key
        options:
          - "-o StrictHostKeyChecking=no"
      - type: ssh 
        host: target.internal
        user: app-user
        env:
          DEPLOYMENT_ENV: production
      - type: docker
        container: app-container
        working_dir: /app
        user: app
    command:
      binary: node
      args: ["server.js", "--port", "8080"]
```

## Layer Types

### Local Layer
Executes commands in the local environment with custom environment and working directory.

```yaml
- type: local
  env:
    PATH: /usr/local/bin:/usr/bin:/bin
  working_dir: /workspace
```

### SSH Layer
Executes commands via SSH to a remote host.

```yaml
- type: ssh
  host: remote.example.com
  user: deploy
  port: 22
  identity_file: ~/.ssh/id_ed25519
  env:
    REMOTE_ENV: production
  options:
    - "-o StrictHostKeyChecking=no"
    - "-o UserKnownHostsFile=/dev/null"
```

### Docker Layer
Executes commands within a Docker container.

```yaml
- type: docker
  container: my-app
  user: app
  working_dir: /app
  env:
    NODE_ENV: production
  interactive: false
  tty: false
```

## Common Use Cases

### Multi-hop SSH (Jump Host)

```yaml
services:
  internal-service:
    type: layered
    layers:
      - type: ssh
        host: bastion.company.com
        user: jump
        port: 22
      - type: ssh
        host: internal.server
        user: app
    command:
      binary: systemctl
      args: ["start", "myapp"]
```

### Remote Docker Execution

```yaml
services:
  containerized-app:
    type: layered
    layers:
      - type: ssh
        host: docker-host.example.com
        user: deploy
      - type: docker
        container: nginx
        user: www-data
        working_dir: /var/www
    command:
      binary: nginx
      args: ["-g", "daemon off;"]
```

### Local Docker with Custom Environment

```yaml
services:
  local-container:
    type: layered
    layers:
      - type: local
        env:
          DOCKER_HOST: unix:///var/run/docker.sock
      - type: docker
        container: redis
        working_dir: /data
    command:
      binary: redis-server
      args: ["--bind", "0.0.0.0"]
```

## Implementation Details

### Layer Composition
Layers are applied in order, with each layer building upon the previous execution context:

1. **Local Layer**: Sets up local environment and working directory
2. **SSH Layer**: Establishes SSH connection, inheriting environment from previous layers
3. **Docker Layer**: Executes within container, with environment from all previous layers

### Environment Variable Handling
- Each layer can define its own environment variables
- Global environment from the service configuration is applied to all layers
- Layer-specific environment variables take precedence over global ones
- Environment variables cascade through layers (SSH inherits from Local, Docker inherits from SSH)

### Process Management
The LayeredServiceExecutor maintains process handles for proper lifecycle management:

- **Start**: Builds the layered executor and executes the command through all layers
- **Stop**: Uses the process handle to properly terminate processes across all layers
- **Health Check**: Supports custom health checks or defaults to process existence
- **Event Streaming**: Provides event streams from the running process

### Error Handling
Errors can occur at any layer in the stack:
- **Configuration errors**: Invalid layer configurations
- **Connection errors**: SSH connection failures, Docker container issues
- **Execution errors**: Command execution failures within the layered context

## Comparison with RemoteSSH Executor

| Feature | RemoteSSH Executor | LayeredServiceExecutor |
|---------|-------------------|------------------------|
| Architecture | Hard-coded SSH over Local | Flexible layer composition |
| Layers | Fixed: Local + SSH | Configurable: Local, SSH, Docker |
| Multi-hop SSH | ❌ Not supported | ✅ Supported |
| Remote Docker | ❌ Not supported | ✅ Supported |
| Configuration | Simple Remote target | Flexible Layered target |
| Use Cases | Direct SSH execution | Complex execution scenarios |

## Testing

The LayeredServiceExecutor includes comprehensive tests:

- **Unit tests**: Layer configuration, serialization, executor logic
- **Integration tests**: Multi-layer execution scenarios
- **Configuration tests**: YAML serialization/deserialization

Run tests with:
```bash
cargo test -p service-orchestration --features smol --lib layered
```

## Registry Integration

The LayeredServiceExecutor is automatically registered in the ExecutorRegistry as "layered" and can handle `ServiceTarget::Layered` configurations.

## Limitations

- **Complexity**: More complex than single-layer executors
- **Debugging**: Multi-layer failures can be harder to diagnose
- **Performance**: Additional overhead from layer composition
- **Dependencies**: Requires all layer dependencies (SSH, Docker) to be available

## Future Enhancements

Potential improvements include:
- **Layer validation**: Pre-flight checks for layer compatibility
- **Layer caching**: Reuse established connections for multiple services
- **Advanced error recovery**: Retry mechanisms for transient layer failures
- **Performance optimization**: Connection pooling and multiplexing
- **Additional layer types**: Support for Kubernetes, systemd, etc.