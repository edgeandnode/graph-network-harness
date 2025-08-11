# SSH Execution via Layered Executor

**Note: The dedicated RemoteSSH executor has been removed in favor of the more flexible LayeredServiceExecutor with SSH layers.**

## Current Implementation

SSH execution is now handled by the `LayeredServiceExecutor` which supports:
- SSH key authentication
- SSH agent forwarding  
- Custom SSH ports and options
- Environment variable forwarding
- **Multi-hop SSH (jump hosts)**
- **SSH + Docker execution**
- **Arbitrary layer composition**

## Configuration Example

```yaml
services:
  my-service:
    target:
      type: layered
      layers:
        - type: ssh
          host: "192.168.1.100"
          user: "deploy"
          port: 22
          identity_file: "/home/user/.ssh/id_rsa"
        - type: local
          working_dir: "/opt/services"
    command:
      binary: "/usr/bin/my-service"
      args: ["--config", "/etc/my-service/config.yaml"]
    env:
      SERVICE_ENV: "production"
```

## Advanced Scenarios

### Multi-hop SSH (Jump Host)
```yaml
services:
  remote-service:
    target:
      type: layered
      layers:
        - type: ssh
          host: "jumphost.example.com"
          user: "admin"
        - type: ssh  
          host: "internal-server"
          user: "deploy"
        - type: local
```

### SSH + Docker Execution
```yaml
services:
  containerized-service:
    target:
      type: layered
      layers:
        - type: ssh
          host: "docker-host.example.com" 
          user: "deploy"
        - type: docker
          container: "my-service-container"
          user: "appuser"
        - type: local
```

### SSH into Test Container
```yaml
services:
  test-service:
    target:
      type: layered  
      layers:
        - type: ssh
          host: "graph-container"
          user: "root"
          port: 2222
          identity_file: "~/.ssh/graph_test_key"
        - type: local
          working_dir: "/opt/graph"
    command:
      binary: "systemctl"
      args: ["start", "my-service"]
```

## Migration from RemoteSSH

Old configuration:
```yaml
services:
  my-service:
    type: remote-ssh
    host: "192.168.1.100"
    user: "deploy"
    binary: "/usr/bin/my-service"
    args: ["--config", "/etc/config.yaml"]
```

New configuration:
```yaml
services:
  my-service:
    target:
      type: layered
      layers:
        - type: ssh
          host: "192.168.1.100"
          user: "deploy"
        - type: local
    command:
      binary: "/usr/bin/my-service" 
      args: ["--config", "/etc/config.yaml"]
```

## Benefits of Layered Approach

1. **Composability** - Mix and match execution layers
2. **Flexibility** - Support complex deployment scenarios
3. **Reusability** - Layer configurations can be shared
4. **Extensibility** - Easy to add new layer types
5. **Testing** - Simplified testing with container-based execution

The layered executor architecture enables much more powerful remote execution scenarios while maintaining simplicity for basic SSH use cases.