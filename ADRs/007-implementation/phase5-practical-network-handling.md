# Practical Network Handling for Phase 5

## Simplified Approach

Given the complexity of network state changes, Phase 5 should focus on detection and user guidance rather than automatic recovery.

## Phase 5: Basic Detection & Guidance

### What We'll Actually Implement

1. **Single Status Command Shows Everything**
   ```bash
   $ harness status
   ┌─────────────┬─────────┬────────────┬─────────┬────────────┬──────────────┐
   │ Service     │ Status  │ Health     │ Network │ Host       │ Issue        │
   ├─────────────┼─────────┼────────────┼─────────┼────────────┼──────────────┤
   │ postgres    │ running │ ✓ healthy  │ local   │ localhost  │              │
   │ redis       │ running │ ✓ healthy  │ local   │ localhost  │              │
   │ api         │ running │ ✗ failed   │ lan     │ 192.168.1.100 │ connection refused │
   │ worker      │ running │ ✗ failed   │ wg      │ 10.0.0.10  │ no route to host │
   │ frontend    │ stopped │ -          │ lan     │ 192.168.1.101 │              │
   └─────────────┴─────────┴────────────┴─────────┴────────────┴──────────────┘
   
   ⚠ 2 services are unreachable (possible network issue)
   
   Suggested actions:
     • Restart affected services: harness restart api worker
     • Check network connectivity: ping 192.168.1.100
     • Full restart: harness restart --all
   ```

2. **Real-time Health Checks**
   - Status command always runs fresh health checks
   - Shows specific error for each service
   - Provides actionable suggestions

3. **Manual Recovery**
   - Users handle network issues as they understand them best
   - Clear error messages guide recovery
   - No magic, no surprises

### What We WON'T Implement

1. **Automatic network state tracking** - Too complex, OS-specific
2. **Automatic reconnection** - Can cause more problems than it solves  
3. **VPN detection** - Too many VPN types and configurations
4. **Sleep/wake detection** - Platform-specific, unreliable
5. **Connection pooling** - Premature optimization

## Practical Error Messages

### SSH Connection Failed
```
Error: Failed to connect to worker@192.168.1.100
  Connection refused

Possible causes:
  • Network changed (laptop moved, VPN disconnected)
  • Remote host is down
  • SSH service not running

Try:
  • Check network: ping 192.168.1.100
  • Restart service: harness restart worker
  • Check SSH manually: ssh worker@192.168.1.100
```

### Service Unreachable
```
Error: Service 'api' health check failed
  Failed to connect to http://192.168.1.100:8080/health

The service process is running but not reachable.
This often happens after network changes.

Try restarting the service:
  harness restart api
```

## Simple Recovery Workflow

1. **User notices something is wrong**
   ```bash
   $ harness logs api
   Error: Cannot connect to remote service
   ```

2. **Check status**
   ```bash
   $ harness status
   [Shows what's running and what's not healthy]
   ```

3. **User decides what to do**
   - Restart specific services
   - Restart everything
   - Debug manually

## Configuration for Resilience

Users can configure services to be more resilient:

```yaml
services:
  remote-api:
    type: remote
    # Shorter timeout for faster failure detection
    connection_timeout: 10
    # More retries for transient issues
    health_check:
      retries: 5
      interval: 30
```

## Future Phases (Not Phase 5)

### Phase 6: Smart Detection
- Basic network change detection
- Connection health monitoring
- Suggested recovery actions

### Phase 7: Automatic Recovery
- Connection pooling
- Automatic reconnection attempts
- VPN-aware service management

## Why This Approach?

1. **Simplicity** - Phase 5 is already complex enough
2. **Reliability** - No magic that might fail mysteriously
3. **User Control** - Users know their network setup best
4. **Clear Errors** - Good error messages > automatic recovery
5. **Incremental** - Can add smarter features later

## Implementation Priority

1. ✅ Clear error messages when connections fail
2. ✅ Health check that detects unreachable services
3. ✅ Simple restart commands that work
4. ❌ Network state tracking (defer)
5. ❌ Automatic recovery (defer)
6. ❌ Connection pooling (defer)
