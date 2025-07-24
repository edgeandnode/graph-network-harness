# Startup Timeout Behavior

## Overview

This document defines how the harness handles service startup timeouts.

## Timeout Configuration

### Precedence (highest to lowest)
1. CLI flag: `harness start api --timeout 600`
2. Service-specific: `startup_timeout: 300` in services.yaml
3. Global default: `settings.startup_timeout: 300`
4. System default: 300 seconds (5 minutes)

## What Happens When Timeout is Exceeded

### Default Behavior

When a service fails to become healthy within the startup timeout:

1. **Service remains running** - The process/container is NOT killed
2. **Command fails** - `harness start` returns non-zero exit code
3. **Error is reported** - Clear message about timeout
4. **Dependent services blocked** - Services depending on the unhealthy service won't start

Example:
```bash
$ harness start api
Starting services...
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ✗ api         [============================] 100% (timeout after 300s)
  
Error: Service 'api' failed to become healthy within 300 seconds

The service is still running but not healthy. You can:
  • Check logs: harness logs api --tail 100
  • Increase timeout: harness start api --timeout 600
  • Force start: harness start api --force
  • Stop service: harness stop api

$ echo $?
3  # Exit code indicates service error
```

### With --force Flag

The `--force` flag changes the behavior:

```bash
$ harness start api --force
Starting services...
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ⚠ api         [============================] 100% (started, health check failed)
  
Warning: Service 'api' started but is not healthy
  • Check status: harness status api
  • View logs: harness logs api

$ echo $?
0  # Success despite unhealthy service
```

### With --no-wait Flag

The `--no-wait` flag skips health checking entirely:

```bash
$ harness start api --no-wait
Starting services...
  → postgres    (started)
  → api         (started)
  
Services started. Check status with: harness status

$ echo $?
0  # Success, health not checked
```

## Status After Timeout

```bash
$ harness status
┌─────────────┬─────────┬────────────┬─────────┬──────────────┐
│ Service     │ Status  │ Health     │ Network │ Uptime       │
├─────────────┼─────────┼────────────┼─────────┼──────────────┤
│ postgres    │ running │ ✓ healthy  │ local   │ 5m 23s       │
│ api         │ running │ ✗ unhealthy│ lan     │ 5m 20s       │
│ worker      │ stopped │ - blocked  │ wg      │ -            │
└─────────────┴─────────┴────────────┴─────────┴──────────────┘

Note: 'worker' is blocked because dependency 'api' is unhealthy
```

## Handling Different Timeout Scenarios

### 1. Service Eventually Becomes Healthy

If a service becomes healthy after the timeout but while still running:
- Status changes from "unhealthy" to "healthy"
- Dependent services can now be started
- No automatic cascade - user must manually start dependents

### 2. Service Needs More Time (Common)

For services with long initialization (database migrations, etc.):

```yaml
services:
  api:
    type: process
    startup_timeout: 900  # 15 minutes for migrations
    health_check:
      start_period: 120   # Don't check for first 2 minutes
      interval: 30
      retries: 5
```

### 3. Service Has No Health Check

Services without health checks are considered healthy immediately:

```yaml
services:
  background-worker:
    type: process
    # No health_check defined
    startup_timeout: 30  # Only waits for process to start
```

### 4. Cascading Timeouts

When multiple services timeout:

```bash
$ harness start --all
Starting services...
  ✓ postgres    [============================] 100% (healthy in 3.2s)
  ✗ api         [============================] 100% (timeout after 300s)
  - worker      [                            ] 0% (blocked)
  - frontend    [                            ] 0% (blocked)

Error: Service startup failed
  ✗ api: timeout after 300s
  - worker: blocked by unhealthy dependency 'api'
  - frontend: blocked by unhealthy dependency 'api'

1 service failed, 2 services blocked
```

## Best Practices

1. **Set realistic timeouts**
   ```yaml
   # Quick services
   redis:
     startup_timeout: 30
   
   # Slow services
   database:
     startup_timeout: 600  # Migrations can be slow
   ```

2. **Use start_period for slow initializers**
   ```yaml
   health_check:
     start_period: 120  # Don't check for 2 minutes
     interval: 10       # Then check every 10s
   ```

3. **Handle timeout in scripts**
   ```bash
   if ! harness start api; then
     echo "API failed to start, checking logs..."
     harness logs api --tail 50
     exit 1
   fi
   ```

4. **Monitor startup progress**
   ```bash
   # In another terminal
   harness logs api -f
   ```

## Implementation Notes

- Timeouts use wall-clock time, not CPU time
- Health checks run in parallel with timeout counter
- Timeout includes time waiting for dependencies
- Each service has independent timeout tracking
- Signals (SIGTERM) are handled gracefully during startup