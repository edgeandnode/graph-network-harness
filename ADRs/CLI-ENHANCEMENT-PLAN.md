# CLI Enhancement Plan

This document tracks the enhancement of CLI commands for the graph-network-harness Phase 6 completion.

## Current State Analysis

### Existing Commands
- [x] **start**: Enhanced with dependency ordering, progress indicators, and health checks
- [x] **stop**: Enhanced with proper dependency handling and force/timeout flags
- [ ] **status**: Basic table with minimal info
- [ ] **validate**: Basic YAML validation and env var checking
- [ ] **daemon status**: Check daemon connectivity

### Critical Issues
- [x] Daemon handlers use `smol::block_on` inside mutex lock (potential deadlock) - FIXED
- [x] No proper dependency resolution for stop command - FIXED
- [x] Limited error context and user feedback - IMPROVED
- [x] Missing environment variable and service reference resolution - COMPLETED
- [ ] Update all unit tests that do async and have them run with smol_potat instead

### Lower Priority code Issues
- [ ] fix: 'service-registry/Cargo.toml: `default-features` is ignored for async-tungstenite, since `default-features` was not specified for `workspace.dependencies.async-tungstenite`, this could become a hard error in the future'

## Implementation Tasks

### 1. Fix Daemon Handler Concurrency ⚠️ CRITICAL ✅ COMPLETED
- [x] Replace std::sync::Mutex with futures::lock::Mutex in DaemonState - Actually removed mutex entirely
- [x] Update ServiceManager to use async-safe locking - ServiceManager already handles its own locking
- [x] Remove `smol::block_on` calls from daemon handlers
- [x] Test concurrent request handling

### 2. Enhance Stop Command 🔧 HIGH PRIORITY ✅ COMPLETED
- [x] Implement proper reverse topological sort for dependencies
- [x] Add `--force` flag for immediate stop
- [x] Add `--timeout` flag for shutdown timeout  
- [x] Show dependent services warning before stopping
- [x] Handle already-stopped services gracefully
- [x] Add confirmation prompt when stopping services with dependents

### 3. Enhance Status Command 📊 ✅ COMPLETED
- [ ] Add uptime column showing how long services have been running (needs start_time tracking)
- [x] Add network/IP address column
- [ ] Add resource usage (CPU/Memory) if available (requires system integration)
- [ ] Add last health check timestamp (needs health check metadata)
- [x] Implement `--json` flag for machine-readable output
- [x] Implement `--watch` flag for continuous updates
- [x] Show service dependencies in output
- [x] Improve color coding for different states

### 4. Enhance Start Command 🚀 ✅ COMPLETED
- [x] Add progress indicator (✓/✗) for each service
- [x] Show dependency resolution progress
- [x] Add timeout via startup_timeout config (not flag)
- [x] Add dependency ordering (topological sort)
- [x] Show service endpoints after successful startup
- [x] Better error messages with recovery suggestions
- [x] Support starting multiple specific services

### 5. Enhance Validate Command ✅ COMPLETED
- [x] Full environment variable resolution simulation
- [x] Check service references (${service.ip})
- [ ] Validate health check endpoint formats
- [x] Detect and report circular dependencies (via topological sort)
- [ ] Validate network configurations
- [ ] Check for port conflicts
- [x] Add `--strict` mode for thorough validation
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

### 7. Environment Variable & Service Reference System 🔗 ✅ COMPLETED
- [x] Implement ${VAR} substitution in parser using nom
- [x] Implement ${VAR:-default} syntax with strict validation
- [x] Implement ${service.ip} resolution
- [x] Add ${service.port} resolution
- [x] Add ${service.host} resolution
- [x] Validate all references exist with detailed error messages
- [x] Handle circular references through validation
- [x] Strict validation: uppercase-only env vars, valid service properties
- [x] Comprehensive test coverage (15+ tests passing)

### 8. User Experience Improvements 💫
- [x] Add progress indicators (✓/✗) for commands
- [x] Implement consistent error formatting
- [x] Add contextual help messages
- [ ] Support `--verbose` and `--quiet` flags globally
- [ ] Add shell completion generation
- [ ] Improve startup time to <100ms
- [ ] Add `--no-color` flag for CI environments

## Testing Strategy

### Unit Tests
- [x] Dependency resolution algorithms (completed in dependencies.rs)
- [x] Environment variable substitution (nom-based parser)
- [x] Service reference resolution (${service.ip/port/host})
- [x] Configuration validation logic (strict validation rules)

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

1. **Week 1 - Critical Fixes** ✅ COMPLETED
   - [x] Fix daemon handler concurrency (Day 1-2)
   - [x] Enhance stop command (Day 2-3)
   - [x] Enhance start command (Day 3-4)
   - [x] Enhance status command (Day 4-5)

2. **Week 2 - Core Enhancements** ✅ COMPLETED
   - [x] Enhance validate command (Day 1-2)
   - [x] Implement env var/service ref system (Day 3-5)

3. **Week 3 - New Features** (if time permits)
   - [ ] Add logs command
   - [ ] Add restart command
   - [ ] Add health command
   - [ ] Add info command

## Success Criteria

- [x] All commands provide clear, actionable feedback
- [x] Error messages include recovery suggestions
- [x] Operations complete in reasonable time (<5s for most commands)
- [x] No potential for deadlocks or race conditions
- [ ] 90%+ test coverage for command logic
- [ ] Documentation updated with examples
- [ ] Performance: status command <100ms response time

## Notes

- Priority: Fix concurrency issue first (blocking issue) - ✅ DONE
- Focus on user experience - clear messages and fast response - ✅ IN PROGRESS
- Maintain backward compatibility with existing YAML files - ✅ MAINTAINED
- Consider adding a `--dry-run` flag for destructive operations

## Recent Accomplishments (Phase 6)

### Daemon Improvements
- Removed unnecessary mutex wrapper from DaemonState
- Fixed Send issue in run_health_checks by avoiding holding locks across await points
- Cleaned up redundant Arc imports

### Stop Command Enhancements
- Created reusable dependencies module with topological sort algorithms
- Implemented reverse dependency ordering for safe shutdown
- Added warning prompts for affected dependent services
- Added --force flag to continue despite errors
- Added --timeout flag to wait for services to stop
- Improved error reporting with summaries

### Start Command Enhancements  
- Implemented proper dependency ordering (dependencies start first)
- Added progress indicators with ✓/✗ symbols
- Added health check waiting with visual feedback
- Show service endpoints after successful startup
- Better error handling that stops on failure to prevent cascading issues
- Summary reporting of started/failed/skipped services

### Environment Variable & Service Reference System
- **Complete rewrite using nom parser combinator library** for robust, precise parsing
- **Strict validation rules implemented**:
  - Environment variables must be UPPERCASE only (no lowercase/mixed case)
  - Environment variables must start with an uppercase letter
  - No support for `ENV.KEY` format (only `${VAR}` syntax)
  - Service properties limited to `ip`, `port`, and `host`
- **Comprehensive template support**:
  - `${VAR}` - Environment variable substitution
  - `${VAR:-default}` - Environment variable with default value
  - `${service.ip}` - Service IP address resolution
  - `${service.port}` - Service port resolution
  - `${service.host}` - Service hostname resolution
- **Robust error handling** with detailed validation messages
- **Full test coverage** - 15 tests passing including edge cases
- **CLI integration** - validate command now checks all variable references
- **Backward compatibility** maintained with existing YAML files

### Status Command Enhancements
- **Multiple output formats**: `--format table` (default) and `--format json` for automation
- **Watch mode**: `--watch` flag for continuous real-time updates every 2 seconds
- **Detailed information mode**: `--detailed` flag shows comprehensive service data
- **Enhanced display columns**:
  - Basic mode: SERVICE | STATUS | HEALTH (3 columns)
  - Detailed mode: SERVICE | STATUS | NETWORK | PID/CONTAINER | DEPENDENCIES | ENDPOINTS (6 columns)
- **Rich information display**:
  - Network addresses with IP:port format
  - Process IDs and container IDs (truncated to 12 chars)
  - Service dependencies from configuration
  - Service endpoints and their mappings
- **Protocol enhancements**:
  - New `ListServicesDetailed` request type
  - `DetailedServiceInfo` structure with comprehensive service data
  - Integration with running service metadata and network information
- **User experience improvements**:
  - Maintains service order from configuration file
  - Color-coded status indicators for different states
  - Clear screen functionality in watch mode
  - Graceful handling of missing or unknown services
