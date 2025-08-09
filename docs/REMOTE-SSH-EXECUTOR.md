# RemoteSSH Executor

The RemoteSSH executor enables running services on remote hosts via SSH.

## Current Implementation

The RemoteSSH executor currently supports:
- SSH key authentication (via `SSH_IDENTITY_FILE` or `SSH_KEY_PATH`)
- SSH agent forwarding
- Custom SSH ports and options
- Environment variable forwarding
- Process execution on remote hosts

## Configuration Example

```yaml
services:
  my-service:
    type: remote-ssh
    host: "192.168.1.100"
    user: "deploy"
    binary: "/usr/bin/my-service"
    args: ["--config", "/etc/my-service/config.yaml"]
    env:
      SSH_PORT: "22"
      SSH_IDENTITY_FILE: "/home/user/.ssh/id_rsa"
      SERVICE_ENV: "production"
```

## Current Limitations

### Single Layer Execution
The RemoteSSH executor is currently hard-coded to use only a LocalLauncher with an SSH layer:

```rust
let executor = LayeredExecutor::new(LocalLauncher)
    .with_layer(ssh_layer);
```

This means it cannot currently support:
- Multi-hop SSH (jump hosts)
- SSH + Docker execution
- SSH + other execution layers

### Future Improvements

To support more complex remote execution scenarios, we could:

1. **Extend RemoteMode** to support layered configurations:
```yaml
services:
  complex-service:
    type: remote-ssh
    host: "jump.example.com"
    user: "jump-user"
    mode:
      type: ssh  # SSH to another host
      host: "target.internal"
      user: "app-user"
      mode:
        type: docker
        container: "my-app"
        command: ["python", "app.py"]
```

2. **Create a LayeredServiceTarget** that explicitly defines execution layers:
```yaml
services:
  layered-service:
    type: layered
    layers:
      - type: ssh
        host: "remote.example.com"
        user: "deploy"
      - type: docker
        container: "app-container"
    command:
      binary: "node"
      args: ["server.js"]
```

3. **Make RemoteSSH executor configurable** to accept custom launchers or additional layers.

## Testing

The RemoteSSH executor includes comprehensive integration tests that use Docker containers with SSH servers. To run the tests:

1. Generate SSH test keys:
```bash
./scripts/generate-ssh-test-keys.sh
```

2. Run the tests (requires Docker):
```bash
cargo test -p service-orchestration --features ssh-tests,docker-tests
```

## Security Considerations

- SSH keys should never be committed to version control
- Use SSH agent forwarding carefully in production
- Consider using dedicated SSH keys for service orchestration
- Enable strict host key checking in production environments