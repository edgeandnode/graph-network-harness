# ADR-008: Service Deployment and Packaging System

## Status

Proposed

## Context

Services in the test harness can run in multiple environments:
- Local native processes (development binaries)
- Docker containers (pre-built images)
- Remote Linux machines (via SSH deployment)

For remote deployment, we need to:
- Package service binaries with their dependencies
- Transfer packages to remote hosts
- Set up the runtime environment
- Manage service lifecycle (start, stop, logs)
- Configure networking (WireGuard mesh)

Currently, there's no standardized way to package and deploy services across these different environments.

## Decision

Implement a service packaging and deployment system that:
1. Creates self-contained service packages (binary + setup scripts + configs)
2. Deploys via SSH with minimal remote dependencies
3. Supports both simple process execution and systemd management
4. Automatically configures WireGuard mesh networking
5. Provides unified lifecycle management across all runtime types

## Consequences

### Positive Consequences

- **Consistent deployment**: Same service definition works locally and remotely
- **Minimal dependencies**: Remote hosts only need SSH and basic Linux tools
- **Development flexibility**: Easy to deploy development builds to remote machines
- **Resource utilization**: Can leverage powerful remote machines for heavy services
- **Testing scenarios**: Enable distributed testing across multiple machines

### Negative Consequences

- **Binary compatibility**: Must ensure binaries work on target architecture/OS
- **Package size**: Shipping binaries increases deployment time
- **SSH dependency**: Requires SSH access and credentials management

### Risks

- **Security**: SSH key management and binary distribution
- **Network reliability**: Deployment failures due to network issues
- **State management**: Handling partial deployments or failures

## Implementation

### Package Structure
```
service-package.tar.gz
├── bin/
│   └── service-binary
├── setup.sh              # Environment setup script
├── config-template.toml   # Service configuration template
├── systemd.service        # Optional systemd unit
└── metadata.json          # Package metadata
```

### Service Definition
```yaml
services:
  indexer-agent:
    runtime: remote
    hosts:
      - address: 192.168.1.100
        ssh_key: ~/.ssh/id_rsa
    
    build:
      type: cargo
      target: x86_64-unknown-linux-gnu
      features: [production]
    
    deploy:
      lifecycle: systemd  # or 'simple'
      setup_commands:
        - "apt-get install -y libssl-dev"
      env:
        DATABASE_URL: "postgres://postgres.mesh.local:5432"
```

### Deployment Flow
```rust
pub struct RemoteDeploymentManager {
    packages: PackageCache,
    sessions: HashMap<String, SshSession>,
    deployments: HashMap<String, DeployedService>,
}

impl RemoteDeploymentManager {
    pub async fn deploy(&mut self, service: &ServiceDef) -> Result<DeploymentInfo> {
        // 1. Build/retrieve package
        let package = self.build_package(service).await?;
        
        // 2. For each remote host
        for host in &service.hosts {
            // 3. Transfer package
            self.transfer_package(&host, &package).await?;
            
            // 4. Setup environment
            self.setup_environment(&host, service).await?;
            
            // 5. Configure networking
            self.setup_wireguard(&host, service).await?;
            
            // 6. Start service
            self.start_service(&host, service).await?;
        }
        
        Ok(DeploymentInfo { ... })
    }
}
```

### Implementation Steps

- [ ] Design package format and metadata structure
- [ ] Implement package builder for different build types
- [ ] Create SSH-based deployment manager
- [ ] Add systemd unit generation for managed services
- [ ] Implement remote WireGuard configuration
- [ ] Add deployment health checks and rollback
- [ ] Create package caching for faster redeployment

## Alternatives Considered

### Alternative 1: Container-only deployment
- Description: Deploy everything as Docker containers
- Pros: Consistent environment, existing tooling
- Cons: Requires Docker on remote hosts, container overhead
- Why rejected: Want to support native binaries for performance

### Alternative 2: Configuration management tools
- Description: Use Ansible/Puppet/Chef for deployment
- Pros: Mature tools, lots of features
- Cons: External dependencies, complex setup
- Why rejected: Want self-contained solution, minimal dependencies

### Alternative 3: Build on remote
- Description: Transfer source code and build on target
- Pros: Guaranteed compatibility
- Cons: Requires build tools on remote, slow deployment
- Why rejected: Want fast deployment, minimal remote dependencies

### Alternative 4: Kubernetes/Nomad
- Description: Use orchestration platform
- Pros: Feature-rich, production-ready
- Cons: Complex setup, overkill for testing
- Why rejected: Too complex for test harness use case