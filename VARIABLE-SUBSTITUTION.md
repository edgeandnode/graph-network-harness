# Variable Substitution System

The harness supports powerful variable substitution in YAML configuration files, allowing dynamic configuration based on environment variables and service references.

## Quick Start

```yaml
services:
  database:
    type: docker
    image: "postgres:15"
    env:
      # Environment variable with default
      POSTGRES_PASSWORD: "${DB_PASSWORD:-secret}"
    ports:
      - 5432

  api:
    type: process
    binary: "./api-server"
    env:
      # Reference another service's IP and port
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD:-secret}@${database.ip}:5432/myapp"
      # Simple environment variable
      LOG_LEVEL: "${LOG_LEVEL}"
```

## Syntax

### Environment Variables

```yaml
# Basic syntax - fails if VAR is not set
"${VAR}"

# With default value - uses "default" if VAR is not set or empty
"${VAR:-default}"
```

### Service References

```yaml
# Reference another service's IP address
"${service_name.ip}"

# Reference another service's first exposed port
"${service_name.port}"

# Reference another service's hostname
"${service_name.host}"
```

## Validation Rules

### Environment Variables
- **MUST be UPPERCASE**: Only uppercase letters, numbers, and underscores allowed
- **MUST start with uppercase letter**: Cannot start with number or underscore
- **No mixed case**: `${MyVar}` is invalid, use `${MYVAR}`
- **No dots in names**: `${ENV.KEY}` format is not supported

Valid examples:
```yaml
"${HOME}"
"${DATABASE_URL}"
"${API_KEY_2}"
"${LOG_LEVEL:-info}"
```

Invalid examples:
```yaml
"${home}"          # Error: lowercase not allowed
"${MyVar}"         # Error: mixed case not allowed
"${2_VAR}"         # Error: cannot start with number
"${_VAR}"          # Error: cannot start with underscore
"${ENV.KEY}"       # Error: dots not allowed in env var names
```

### Service References
- **Service must exist**: Referenced service must be defined in the YAML
- **Property must be valid**: Only `ip`, `port`, and `host` are supported
- **Case sensitive**: Service names are case sensitive

Valid examples:
```yaml
"${postgres.ip}"
"${api_server.port}"
"${worker_node.host}"
```

Invalid examples:
```yaml
"${postgres.url}"      # Error: 'url' is not a valid property
"${unknown.ip}"        # Error: service 'unknown' not defined
"${postgres.IP}"       # Error: property must be lowercase
```

## Complete Examples

### Database Connection String
```yaml
services:
  postgres:
    type: docker
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
      POSTGRES_USER: "${DB_USER:-postgres}"
      POSTGRES_DB: "${DB_NAME:-myapp}"
    ports:
      - 5432

  api:
    type: process
    binary: "./api-server"
    env:
      # Full connection string with references
      DATABASE_URL: "postgresql://${DB_USER:-postgres}:${DB_PASSWORD}@${postgres.ip}:5432/${DB_NAME:-myapp}"
    dependencies:
      - postgres
```

### Multi-Service Setup
```yaml
services:
  redis:
    type: docker
    image: "redis:7"
    ports:
      - 6379

  postgres:
    type: docker
    image: "postgres:15"
    env:
      POSTGRES_PASSWORD: "${DB_PASSWORD}"
    ports:
      - 5432

  api:
    type: process
    binary: "./api-server"
    env:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD}@${postgres.ip}:5432/api"
      REDIS_URL: "redis://${redis.ip}:6379"
      API_PORT: "${API_PORT:-8080}"
    dependencies:
      - postgres
      - redis

  worker:
    type: process
    binary: "./worker"
    env:
      API_URL: "http://${api.host}:${API_PORT:-8080}"
      REDIS_URL: "redis://${redis.ip}:6379"
      WORKER_THREADS: "${WORKER_THREADS:-4}"
    dependencies:
      - api
      - redis
```

### Remote Service Deployment
```yaml
networks:
  lan:
    type: lan
    subnet: "192.168.1.0/24"
    nodes:
      - host: "192.168.1.100"
        ssh_user: "deploy"

services:
  metrics-db:
    type: docker
    network: local
    image: "prometheus:latest"
    ports:
      - 9090

  monitoring-agent:
    type: remote
    network: lan
    host: "192.168.1.100"
    binary: "/opt/agent/bin/monitor"
    env:
      # Reference service on different network
      METRICS_ENDPOINT: "http://${metrics-db.ip}:9090/api/v1/write"
      AGENT_NAME: "${HOSTNAME:-monitoring-1}"
      SCRAPE_INTERVAL: "${SCRAPE_INTERVAL:-30s}"
    dependencies:
      - metrics-db
```

## CLI Validation

The `harness validate` command checks all variable references:

```bash
# Basic validation
harness validate

# Strict validation - fails on missing environment variables
harness validate --strict
```

Example validation output:
```
Validating services.yaml...
✓ Network topology valid
✓ Service dependencies form valid DAG
✗ Environment variable validation failed:
  - Missing environment variable: DB_PASSWORD (required by postgres, api)
  - Invalid service reference: ${unknown.ip} (service 'unknown' not found)
  - Invalid property reference: ${redis.url} (property 'url' not valid, use 'ip', 'port', or 'host')
```

## Resolution Order

Variables are resolved in this order:
1. **Service references**: `${service.ip}`, `${service.port}`, `${service.host}`
2. **Environment variables**: From the shell environment
3. **Default values**: The value after `:-` in `${VAR:-default}`

## Best Practices

### 1. Use Meaningful Defaults
```yaml
env:
  LOG_LEVEL: "${LOG_LEVEL:-info}"           # Good: sensible default
  API_KEY: "${API_KEY}"                     # Good: no default for secrets
  WORKERS: "${WORKERS:-1}"                  # Good: safe minimum
```

### 2. Group Related Variables
```yaml
env:
  # Database configuration
  DB_HOST: "${postgres.ip}"
  DB_PORT: "5432"
  DB_NAME: "${DB_NAME:-myapp}"
  DB_USER: "${DB_USER:-postgres}"
  DB_PASSWORD: "${DB_PASSWORD}"
  
  # Application settings
  APP_ENV: "${APP_ENV:-development}"
  APP_PORT: "${APP_PORT:-8080}"
  APP_LOG_LEVEL: "${LOG_LEVEL:-info}"
```

### 3. Document Required Variables
```yaml
# In your README or documentation:
# Required environment variables:
# - DB_PASSWORD: PostgreSQL password
# - API_KEY: External API authentication
# - ETHEREUM_RPC_URL: Ethereum node endpoint
```

### 4. Use Validation During Development
```bash
# Always validate before deployment
harness validate --strict

# Check specific service configurations
harness validate --service api --strict
```

## Common Patterns

### Connection Strings
```yaml
# PostgreSQL
"postgresql://${DB_USER}:${DB_PASSWORD}@${postgres.ip}:5432/${DB_NAME}"

# Redis
"redis://${redis.ip}:6379/${REDIS_DB:-0}"

# MongoDB
"mongodb://${mongo.ip}:27017/${DB_NAME}"

# Elasticsearch
"http://${elasticsearch.ip}:9200"
```

### API Endpoints
```yaml
# Internal service communication
"http://${api.ip}:${API_PORT:-8080}/graphql"

# External endpoints with env override
"${API_ENDPOINT:-http://localhost:8080}"
```

### Feature Flags
```yaml
env:
  ENABLE_METRICS: "${ENABLE_METRICS:-false}"
  ENABLE_TRACING: "${ENABLE_TRACING:-false}"
  DEBUG_MODE: "${DEBUG:-false}"
```

## Troubleshooting

### Missing Environment Variables
```bash
# Check what variables are required
harness validate --show-vars

# Set variables for session
export DB_PASSWORD="secret"
export API_KEY="your-key"

# Or use .env file
cat > .env << EOF
DB_PASSWORD=secret
API_KEY=your-key
EOF
source .env
```

### Invalid Service References
```bash
# List all services and their properties
harness status --detailed

# Check service names in YAML
grep "^  [a-zA-Z]" services.yaml
```

### Debugging Substitution
```bash
# See resolved values (in logs)
harness start --verbose

# Test specific substitutions
echo "Test: ${postgres.ip}" | harness validate --stdin
```

## Limitations

1. **No Nested Substitution**: Cannot use variables within variables
   ```yaml
   # Not supported:
   "${${ENV_PREFIX}_VAR}"
   ```

2. **No Arithmetic Operations**: Variables are strings only
   ```yaml
   # Not supported:
   "${PORT + 1}"
   ```

3. **No Conditional Logic**: Use defaults instead
   ```yaml
   # Instead of conditionals, use:
   "${VAR:-default}"
   ```

4. **Service Must Be Defined First**: Cannot reference services defined later in the file

## Integration with CI/CD

### GitHub Actions
```yaml
- name: Validate configuration
  run: |
    export DB_PASSWORD="${{ secrets.DB_PASSWORD }}"
    export API_KEY="${{ secrets.API_KEY }}"
    harness validate --strict

- name: Deploy services
  run: |
    harness start --all
```

### GitLab CI
```yaml
validate:
  script:
    - export DB_PASSWORD="$DB_PASSWORD"
    - export API_KEY="$API_KEY"
    - harness validate --strict

deploy:
  script:
    - harness start --all
  environment:
    name: production
```

### Docker Compose Integration
```yaml
# Use harness variables in docker-compose.override.yml
version: '3'
services:
  api:
    environment:
      DATABASE_URL: "postgresql://postgres:${DB_PASSWORD}@${postgres.ip}:5432/api"
```