# Phase 4: Service Lifecycle Architecture

## Overview
Phase 4 implements the core orchestration logic that manages heterogeneous services across different execution environments while providing unified networking.

## Core Components

### 1. Service Manager
Central orchestrator that manages the entire service lifecycle.

```rust
pub struct ServiceManager {
    registry: ServiceRegistry,           // From service-registry crate
    network_manager: NetworkManager,     // From service-registry crate
    executors: HashMap<String, Box<dyn ServiceExecutor>>,
    active_services: HashMap<String, RunningService>,
}

impl ServiceManager {
    pub async fn start_service(&mut self, name: &str, config: ServiceConfig) -> Result<()>;
    pub async fn stop_service(&mut self, name: &str) -> Result<()>;
    pub async fn deploy_package(&mut self, target: RemoteTarget, package: Package) -> Result<()>;
    pub async fn get_service_status(&self, name: &str) -> Result<ServiceStatus>;
}
```

### 2. Service Executors
Abstraction layer over command-executor for different service types.

```rust
#[async_trait]
pub trait ServiceExecutor {
    async fn start(&self, config: ServiceConfig) -> Result<RunningService>;
    async fn stop(&self, service: &RunningService) -> Result<()>;
    async fn health_check(&self, service: &RunningService) -> Result<HealthStatus>;
    async fn get_logs(&self, service: &RunningService) -> Result<LogStream>;
}

// Implementations:
pub struct ProcessExecutor(Executor<LocalLauncher>);
pub struct DockerExecutor(Executor<LocalLauncher>);  // Uses Docker target
pub struct RemoteExecutor(Executor<SshLauncher<LocalLauncher>>);
```

### 3. Service Configuration
Configuration model matching ADR-007 specification.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub target: ServiceTarget,
    pub dependencies: Vec<String>,
    pub health_check: Option<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServiceTarget {
    Process { 
        binary: String, 
        args: Vec<String>,
        env: HashMap<String, String>,
        working_dir: Option<String>,
    },
    Docker { 
        image: String,
        env: HashMap<String, String>,
        ports: Vec<u16>,
        volumes: Vec<String>,
    },
    RemoteLan { 
        host: String,
        user: String,
        binary: String,
        args: Vec<String>,
    },
    Wireguard { 
        host: String,
        user: String,
        package: String,  // Path to package tarball
    },
}
```

### 4. Network Integration
Integration with service-registry's network management.

```rust
impl ServiceManager {
    async fn inject_network_config(&self, service: &ServiceConfig) -> Result<ServiceConfig> {
        // 1. Register service with network manager
        let network = self.network_manager.register_service(service).await?;
        
        // 2. Resolve dependency IPs
        let mut env_vars = service.target.env().clone();
        for dep in &service.dependencies {
            let ip = self.network_manager.resolve_service_ip(&service.name, dep).await?;
            env_vars.insert(format!("{}_ADDR", dep.to_uppercase()), ip.to_string());
        }
        
        // 3. Return updated config with network info
        Ok(service.with_env(env_vars))
    }
}
```

### 5. Package Management
Remote deployment using the package format from ADR-007.

```rust
pub struct PackageDeployer {
    ssh_executor: Executor<SshLauncher<LocalLauncher>>,
}

impl PackageDeployer {
    pub async fn deploy(&self, package_path: &str, target: RemoteTarget) -> Result<DeployedPackage> {
        // 1. Transfer package to remote host
        self.transfer_package(package_path, &target).await?;
        
        // 2. Extract package to /opt/harness/<service-name>/
        self.extract_package(&target).await?;
        
        // 3. Generate environment file with dependency IPs
        self.generate_env_file(&target).await?;
        
        // 4. Call start.sh script
        self.start_service(&target).await?;
        
        Ok(DeployedPackage { target, path: format!("/opt/harness/{}", target.service_name) })
    }
}
```

## Implementation Plan

### Step 1: Create Orchestrator Crate
```
crates/orchestrator/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── manager.rs          # ServiceManager
│   ├── executors/          # ServiceExecutor implementations
│   │   ├── mod.rs
│   │   ├── process.rs      # ProcessExecutor  
│   │   ├── docker.rs       # DockerExecutor
│   │   └── remote.rs       # RemoteExecutor
│   ├── config.rs           # ServiceConfig, ServiceTarget
│   ├── package.rs          # PackageDeployer
│   └── health.rs           # Health checking
├── tests/
│   ├── local_services.rs
│   ├── docker_services.rs  
│   └── remote_services.rs
└── examples/
    └── basic_orchestration.rs
```

### Step 2: Basic Local Service Management
- Implement ProcessExecutor using command-executor
- Test with simple local processes
- Integration with service-registry

### Step 3: Docker Service Support  
- Implement DockerExecutor using Docker targets
- Container lifecycle management
- Port mapping and volumes

### Step 4: Remote Service Deployment
- Implement RemoteExecutor using SSH
- Package transfer and extraction
- Remote service management

### Step 5: Network Integration
- IP injection based on network topology
- Service discovery integration
- Environment variable generation

## Key Design Decisions

1. **Layered Architecture**: ServiceManager orchestrates, ServiceExecutors handle specifics
2. **Reuse Existing Components**: Heavy use of command-executor and service-registry
3. **Configuration-Driven**: Services defined in YAML, runtime behavior determined by target type
4. **Network Transparency**: Services receive dependency IPs as environment variables
5. **Package-Based Deployment**: Remote services deployed as self-contained packages

## Success Criteria

- [ ] Local process services can be started/stopped
- [ ] Docker container services work with networking
- [ ] Remote services can be deployed and managed via SSH
- [ ] Network topology detection works end-to-end
- [ ] Service dependencies resolved with correct IPs
- [ ] Health checking and log collection functional
- [ ] Matches ADR-007 services.yaml format exactly