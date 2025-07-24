# Partial Startup Failure Handling

## Problem Statement

When starting multiple services with dependencies, some services may start successfully while others fail. This creates a partially running system that needs careful handling.

## Scenarios

### Scenario 1: Mid-Chain Failure
```
postgres (✓) → api (✗) → worker (blocked) → frontend (blocked)
```

### Scenario 2: Parallel Failure
```
postgres (✓) → api (✓) → worker (✓)
              ↘ cache (✗) → analytics (blocked)
```

### Scenario 3: Timeout During Multi-Service Start
```bash
$ harness start --all
postgres (✓) → api (timeout) → [10 other services blocked]
```

## Design Options

### Option 1: Keep Running (Default) ⭐ RECOMMENDED

Successfully started services continue running, failed services stop, blocked services never start.

```bash
$ harness start --all
Starting services...
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ✓ redis       [============================] 100% (healthy in 0.8s)
  ✗ api         [============================] 100% (failed to start)
  - worker      [                            ] 0% (blocked)

Error: Partial startup failure
  Started: postgres, redis (2 services)
  Failed: api (1 service)  
  Blocked: worker (1 service)

Started services are still running. You can:
  • Stop all: harness stop --all
  • Retry failed: harness start api
  • Force continue: harness start worker --force
```

**Pros:**
- No unnecessary disruption
- Allows debugging with partial system
- User maintains control
- Can retry just the failed parts

**Cons:**
- System in partial state
- May consume resources

### Option 2: Rollback All

Stop all services if any fail, returning to clean state.

```bash
$ harness start --all --rollback-on-failure
Starting services...
  ✓ postgres    [============================] 100% (healthy)
  ✓ redis       [============================] 100% (healthy)
  ✗ api         [============================] 100% (failed)
  
Rolling back due to failure...
  ↶ Stopping redis...
  ↶ Stopping postgres...
  
Error: Startup failed, all services stopped
```

**Pros:**
- Clean state
- No partial system
- Predictable

**Cons:**
- Wastes successful startups
- Longer retry times
- May interrupt debugging

### Option 3: Rollback Dependencies Only

Stop only services that depend on failed service.

```bash
$ harness start --all --smart-rollback
Starting services...
  ✓ postgres    [============================] 100% (healthy)
  ✓ redis       [============================] 100% (healthy)  
  ✗ api         [============================] 100% (failed)
  ✓ metrics     [============================] 100% (healthy)
  
Partial failure detected:
  Keeping independent services: postgres, redis, metrics
  Not starting dependent services: worker, frontend
```

## Proposed Implementation

### Default Behavior: Check & Prompt

1. **Always check current state first** - See what's already running
2. **Prompt on partial state** - Let user decide how to proceed
3. **Provide clear options** - Continue, teardown, or abort
4. **Flags for automation** - Override prompts for scripts

### Pre-Start State Check

```bash
$ harness start --all
Checking current state...
  ✓ postgres    (already running, healthy)
  ✓ redis       (already running, healthy)
  ✗ api         (stopped, last exit: failed)
  - worker      (stopped)
  - frontend    (stopped)

Partial system detected: 2 of 5 services are running.

How would you like to proceed?
  1) Continue - Start remaining services
  2) Restart - Stop all and start fresh  
  3) Teardown - Stop all services
  4) Abort - Do nothing

Choice [1-4]: _
```

### During Startup Failure

```bash
$ harness start --all
Starting services...
  → postgres    [============================] using existing (healthy)
  → redis       [============================] using existing (healthy)
  ✗ api         [============================] failed to start (exit code 1)

Startup failed. Currently running: postgres, redis

How would you like to proceed?
  1) Keep running - Leave successful services running
  2) Teardown - Stop all services
  3) Retry - Try starting 'api' again
  4) Force continue - Start remaining services anyway

Choice [1-4]: _
```

### Override Flags

```bash
# Always continue with partial state
harness start --all --continue

# Always teardown on failure  
harness start --all --teardown-on-failure

# Always restart everything
harness start --all --restart

# Never prompt (use default: continue)
harness start --all --no-interactive
```

### Status After Partial Failure

```bash
$ harness status
┌─────────────┬─────────┬────────────┬─────────────────────┐
│ Service     │ Status  │ Health     │ Notes               │
├─────────────┼─────────┼────────────┼─────────────────────┤
│ postgres    │ running │ ✓ healthy  │                     │
│ redis       │ running │ ✓ healthy  │                     │
│ api         │ stopped │ ✗ failed   │ exit code 1         │
│ worker      │ stopped │ - blocked  │ dependency failed   │
│ frontend    │ stopped │ - blocked  │ dependency failed   │
└─────────────┴─────────┴────────────┴─────────────────────┘

System state: PARTIAL
  2 services running (postgres, redis)
  1 service failed (api)
  2 services blocked (worker, frontend)

To recover:
  harness start api        # Retry failed service
  harness stop --all       # Stop everything
```

### Recovery Commands

```bash
# Retry just failed services
$ harness start --failed
Starting previously failed services...
  ⟳ api         [===>                        ] 15%

# Start blocked services anyway (ignore deps)
$ harness start --blocked --force
Starting blocked services (ignoring dependencies)...
  ⚠ worker      [============================] 100% (started, deps unhealthy)
  ⚠ frontend    [============================] 100% (started, deps unhealthy)

# Clean restart
$ harness restart --all
Stopping all services...
Starting all services...
```

## Transaction Log

Track service state changes for recovery:

```bash
$ harness status --history
2024-01-24 10:15:23  postgres    stopped → starting → running (healthy)
2024-01-24 10:15:26  redis       stopped → starting → running (healthy)  
2024-01-24 10:15:29  api         stopped → starting → stopped (failed: exit 1)
2024-01-24 10:15:29  worker      stopped → blocked (dependency failed: api)
2024-01-24 10:15:29  frontend    stopped → blocked (dependency failed: api)
```

## State Detection for External Services

The harness must handle services it didn't start (e.g., system services, manually started):

```bash
$ harness start api
Checking current state...
  ! postgres    (running, not managed by harness)
  - api         (stopped)

Detected external service: postgres (PID 1234)

How would you like to proceed?
  1) Adopt - Manage this postgres instance
  2) Coexist - Leave it alone and start api
  3) Replace - Stop external postgres and start managed one
  4) Abort - Do nothing

Choice [1-4]: _
```

### Service Adoption

When adopting external services:

```bash
$ harness adopt postgres
Found postgres process (PID 1234):
  Command: /usr/bin/postgres -D /var/lib/postgresql/data
  Port: 5432
  User: postgres

Adopt this as managed service 'postgres'? [y/N]: y
✓ Service 'postgres' adopted

$ harness status
┌─────────────┬─────────┬────────────┬─────────────────────┐
│ Service     │ Status  │ Health     │ Notes               │
├─────────────┼─────────┼────────────┼─────────────────────┤
│ postgres    │ running │ ✓ healthy  │ adopted (PID 1234)  │
└─────────────┴─────────┴────────────┴─────────────────────┘
```

## Best Practices

1. **Use flags in scripts to avoid prompts**
   ```bash
   # CI/CD script
   harness start --all --continue --no-interactive
   ```

2. **Check state before starting**
   ```bash
   # See what's running
   harness status
   
   # Dry run to see what would happen
   harness start --all --dry-run
   ```

3. **Group related services**
   ```bash
   # Start core services first
   harness start postgres redis
   
   # Check they're healthy
   harness health postgres redis
   
   # Then application layer
   harness start api worker
   ```

4. **Handle external services explicitly**
   ```yaml
   services:
     postgres:
       type: ssh  # For external services
       attach:
         process_name: "postgres"
         health_check:
           command: "pg_isready"
   ```

## Implementation Priority

1. **Phase 5.0**: 
   - Pre-start state checking
   - Interactive prompts with choices
   - Basic override flags (--continue, --restart)
   
2. **Phase 5.1**:
   - External service detection
   - Service adoption functionality
   - --dry-run support
   
3. **Phase 5.2**:
   - Advanced recovery modes
   - Transaction log
   - State history tracking
   
4. **Phase 6**:
   - Intelligent failure analysis
   - Automatic recovery suggestions
   - Distributed state management