# Composable Service Orchestration Architecture

## The Power of Runtime Composition

The key insight is that **execution layers are data, not code**. This enables:

### 1. Environment as Configuration

```yaml
# environments.yaml
environments:
  local:
    layers: []  # Direct local execution
  
  dev:
    layers:
      - type: docker
        config:
          container: dev-env
  
  staging:
    layers:
      - type: ssh
        config:
          host: staging.example.com
  
  production:
    layers:
      - type: ssh
        config:
          host: bastion.example.com
      - type: ssh
        config:
          host: prod-internal
      - type: docker
        config:
          container: prod-app
```

### 2. Service Definitions are Environment-Agnostic

```yaml
# services.yaml
services:
  log-processor:
    command: python3 /app/process_logs.py
    stdin_mode: streaming
    
  database-backup:
    command: pg_dump production_db
    stdout_mode: file
    
  health-check:
    command: curl http://localhost:8080/health
```

### 3. Runtime Composition

```rust
// The magic: combine environment + service at runtime
let env = config.get_environment("production");
let service = config.get_service("log-processor");

// Build executor from environment layers
let mut executor = LayeredExecutor::new(LocalLauncher);
for layer in env.layers {
    executor = executor.with_layer(layer.to_executor_layer());
}

// Launch service with stdin support
let cmd = service.to_command();
let (events, handle) = executor.launch(&target, cmd).await?;
```

## How Stdin Flows Through Layers

```
Application Code
    │
    ├─ Direct Write: handle.stdin_mut().write_line("data")
    │
    └─ Channel: tx.send("data").await
                    │
                    ▼
           ┌──────────────┐
           │ StdinHandle  │
           └──────┬───────┘
                  │
    ═══════════════════════════════════
    Layered Execution (Transparent Flow)
    ═══════════════════════════════════
                  │
           ┌──────▼───────┐
           │ Local Layer  │ (env vars, working dir)
           └──────┬───────┘
                  │
           ┌──────▼───────┐
           │  SSH Layer   │ (ssh user@host 'command')
           └──────┬───────┘
                  │
           ┌──────▼───────┐
           │ Docker Layer │ (docker exec container)
           └──────┬───────┘
                  │
           ┌──────▼───────┐
           │   Process    │
           └──────────────┘
```

## Real-World Use Cases

### 1. Log Streaming Pipeline
```yaml
log-streamer:
  command: tail -f /var/log/app.log
  stdout_mode: stream
  filters:
    - type: grep
      pattern: ERROR
    - type: jq
      query: .timestamp,.message
```

Run in any environment:
- Local: reads local logs
- Docker: reads container logs  
- Production: SSH → Docker → tail logs

### 2. Database Operations
```yaml
db-query:
  command: psql -U postgres
  stdin_mode: interactive
  environment:
    PGPASSWORD: "{{secret:db_password}}"
```

### 3. Deployment Scripts
```yaml
deploy:
  pre_commands:
    - command: git pull
    - command: npm install
  command: npm run deploy
  post_commands:
    - command: npm run health-check
```

## Benefits

1. **Write Once, Run Anywhere**: Same service definition works in all environments
2. **No Code Changes**: New environments just need YAML config
3. **Testable**: Test locally, deploy anywhere
4. **Composable**: Mix and match layers as needed
5. **Stdin/Stdout Preservation**: Data flows transparently through all layers

## Implementation Status

- ✅ Layered execution system
- ✅ SSH, Docker, Local layers  
- ✅ Stdin support in base executor
- ⏳ YAML configuration loader
- ⏳ Service lifecycle management
- ⏳ Stdin forwarding through layers
- ⏳ Runtime layer composition from config

## The Key Insight

Traditional approach:
```rust
// Hard-coded for each environment
if env == "prod" {
    ssh_to_bastion()
    then_ssh_to_prod()
    then_docker_exec()
} else if env == "dev" {
    docker_exec_local()
}
```

Composable approach:
```rust
// Layers composed from data
let layers = config.environment.layers;
let executor = layers.into_iter()
    .fold(LayeredExecutor::new(), |ex, layer| ex.with_layer(layer));
```

This is the power of treating **infrastructure as data**!