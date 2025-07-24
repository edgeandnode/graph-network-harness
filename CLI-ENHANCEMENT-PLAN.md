# CLI Enhancement Plan

This document tracks the enhancement of CLI commands for the graph-network-harness Phase 6 completion.

## Current State Analysis

### Existing Commands
- [ ] **start**: Basic implementation with simple progress messages
- [ ] **stop**: Simple reverse order, no real dependency handling  
- [ ] **status**: Basic table with minimal info
- [ ] **validate**: Basic YAML validation and env var checking
- [ ] **daemon status**: Check daemon connectivity

### Critical Issues
- [ ] Daemon handlers use `smol::block_on` inside mutex lock (potential deadlock)
- [ ] No proper dependency resolution for stop command
- [ ] Limited error context and user feedback
- [ ] Missing environment variable and service reference resolution

## Implementation Tasks

### 1. Fix Daemon Handler Concurrency ⚠️ CRITICAL
- [ ] Replace std::sync::Mutex with futures::lock::Mutex in DaemonState
- [ ] Update ServiceManager to use async-safe locking
- [ ] Remove `smol::block_on` calls from daemon handlers
- [ ] Test concurrent request handling

### 2. Enhance Stop Command 🔧 HIGH PRIORITY
- [ ] Implement proper reverse topological sort for dependencies
- [ ] Add `--force` flag for immediate stop
- [ ] Add `--timeout` flag for shutdown timeout  
- [ ] Show dependent services warning before stopping
- [ ] Handle already-stopped services gracefully
- [ ] Add confirmation prompt when stopping services with dependents

### 3. Enhance Status Command 📊
- [ ] Add uptime column showing how long services have been running
- [ ] Add network/IP address column
- [ ] Add resource usage (CPU/Memory) if available
- [ ] Add last health check timestamp
- [ ] Implement `--json` flag for machine-readable output
- [ ] Implement `--watch` flag for continuous updates
- [ ] Show service dependencies in output
- [ ] Improve color coding for different states

### 4. Enhance Start Command 🚀
- [ ] Add progress indicator (spinner) for each service
- [ ] Show dependency resolution progress
- [ ] Add `--timeout` flag for startup timeout
- [ ] Add `--no-deps` flag to skip dependencies
- [ ] Show service endpoints after successful startup
- [ ] Better error messages with recovery suggestions
- [ ] Support starting multiple specific services

### 5. Enhance Validate Command ✅
- [ ] Full environment variable resolution simulation
- [ ] Check service references (${service.ip})
- [ ] Validate health check endpoint formats
- [ ] Detect and report circular dependencies
- [ ] Validate network configurations
- [ ] Check for port conflicts
- [ ] Add `--strict` mode for thorough validation
- [ ] Suggest fixes for common configuration issues

### 6. Add New Commands 🆕
- [ ] **logs** command
  - [ ] Stream logs from services
  - [ ] Support `--follow` flag
  - [ ] Support `--tail` flag
  - [ ] Filter by service name
- [ ] **restart** command  
  - [ ] Graceful stop then start
  - [ ] Preserve service state
  - [ ] Support single or multiple services
- [ ] **health** command
  - [ ] Run health checks on demand
  - [ ] Show detailed health status
  - [ ] Support checking specific services
- [ ] **info** command
  - [ ] Detailed information about a service
  - [ ] Show configuration
  - [ ] Show current state and history

### 7. Environment Variable & Service Reference System 🔗
- [ ] Implement ${VAR} substitution in parser
- [ ] Implement ${VAR:-default} syntax
- [ ] Implement ${service.ip} resolution
- [ ] Add ${service.port} resolution
- [ ] Validate all references exist
- [ ] Handle circular references
- [ ] Cache resolved values for performance

### 8. User Experience Improvements 💫
- [ ] Add progress bars using indicatif
- [ ] Implement consistent error formatting
- [ ] Add contextual help messages
- [ ] Support `--verbose` and `--quiet` flags globally
- [ ] Add shell completion generation
- [ ] Improve startup time to <100ms
- [ ] Add `--no-color` flag for CI environments

## Testing Strategy

### Unit Tests
- [ ] Dependency resolution algorithms
- [ ] Environment variable substitution
- [ ] Service reference resolution
- [ ] Configuration validation logic

### Integration Tests  
- [ ] Mock daemon communication
- [ ] Command success scenarios
- [ ] Error handling scenarios
- [ ] Concurrent command execution

### End-to-End Tests
- [ ] Full YAML -> CLI -> Daemon -> Service flow
- [ ] Multi-service dependency scenarios
- [ ] Network topology scenarios
- [ ] Failure recovery scenarios

## Implementation Order

1. **Week 1 - Critical Fixes**
   - [ ] Fix daemon handler concurrency (Day 1-2)
   - [ ] Enhance stop command (Day 2-3)
   - [ ] Enhance status command (Day 4-5)

2. **Week 2 - Core Enhancements**
   - [ ] Enhance start command (Day 1-2)
   - [ ] Enhance validate command (Day 2-3)
   - [ ] Implement env var/service ref system (Day 4-5)

3. **Week 3 - New Features** (if time permits)
   - [ ] Add logs command
   - [ ] Add restart command
   - [ ] Add health command
   - [ ] Add info command

## Success Criteria

- [ ] All commands provide clear, actionable feedback
- [ ] Error messages include recovery suggestions
- [ ] Operations complete in reasonable time (<5s for most commands)
- [ ] No potential for deadlocks or race conditions
- [ ] 90%+ test coverage for command logic
- [ ] Documentation updated with examples
- [ ] Performance: status command <100ms response time

## Notes

- Priority: Fix concurrency issue first (blocking issue)
- Focus on user experience - clear messages and fast response
- Maintain backward compatibility with existing YAML files
- Consider adding a `--dry-run` flag for destructive operations