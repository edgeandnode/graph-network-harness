# ADR-007 Implementation Plan

## Overview
This document tracks the implementation of ADR-007: Distributed Service Orchestration.

## Phase Summary

| Phase | Name | Status | Duration | Goal |
|-------|------|--------|----------|------|
| 1 | Command Executor | ✅ Complete | 3 weeks | Runtime-agnostic command execution |
| 2 | Service Registry | ✅ Complete | 1 week | Service discovery & WebSocket API |
| 3 | Networking | ✅ Complete* | (merged) | Network topology & IP management |
| 4 | Service Lifecycle | ✅ Complete* | (merged) | Orchestrator with health monitoring |
| 5 | Configuration System | 🚧 Next | 2 weeks | YAML config & minimal CLI |
| 6 | Full CLI Experience | 📋 Planned | 2 weeks | Complete CLI with great UX |
| 7 | Production Features | 🔮 Future | TBD | Scaling, monitoring, enterprise |

*Phases 3 & 4 were implemented together in the orchestrator crate

## Implementation Phases

### Phase 1: Command Executor ✅ COMPLETE
- [x] Create command-executor crate structure
- [x] Implement core types (Executor<T>, ProcessHandle trait)
- [x] Implement LocalLauncher with basic Command/ManagedProcess support
- [x] Add tests for local command execution
- [x] Implement event streaming with line buffering
- [x] Refactor API to split event streams and process handles
- [x] **Architecture Change**: Split into Launcher/Attacher paradigms
  - [x] Create Launcher trait (for things we spawn)
  - [x] Create Attacher trait (for things we connect to)
  - [x] Implement LocalLauncher and LocalAttacher
  - [x] Split traits into separate modules
- [x] Implement ManagedService for generic service attachment
- [x] Add SystemdPortable support to LocalLauncher
- [x] **Nested Architecture**: Implement nested launchers
  - [x] Launchers support nesting (SshLauncher<L>)
  - [x] Docker types moved to target module
  - [x] LocalLauncher handles all target types (Command, ManagedProcess, Docker, Compose)
  - [x] SSH launcher with CLI wrapper implemented
  - [x] SudoLauncher for elevated execution
- [x] Self-hosted tests running with Docker
- [x] Comprehensive test infrastructure created

**Phase 1 Completed on 2025-01-21:**
- [x] API polish and documentation
  - [x] All public APIs have doc comments
  - [x] All modules properly documented
  - [x] Nested launcher pattern documented
- [x] Test stability improvements
  - [x] All integration tests pass reliably
  - [x] Flaky tests properly feature-gated
- [x] Minor cleanup
  - [x] No unused code warnings
  - [x] Consistent error handling throughout
- [x] Created comprehensive README.md with examples

### Phase 2: Core Infrastructure ✅ COMPLETE

Started: 2025-01-21
Completed: 2025-01-22

**Week 1: Registry Core** ✅
- [x] Data models (ServiceEntry, Endpoint, Location)
- [x] In-memory store with thread-safe access
- [x] Service CRUD operations
- [x] State tracking and transitions
- [x] File persistence with async-fs
- [x] Event generation system

**Week 2: WebSocket Server** ✅
- [x] async-tungstenite integration (no runtime features)
- [x] Connection handling with async-net
- [x] Message protocol (request/response/event)
- [x] Event subscription management
- [x] Connection lifecycle and error handling
- [x] Broadcast system for events
- [x] **BONUS**: Full TLS support with rustls

**Week 3: Package Management** ✅
- [x] Package format specification
- [x] Manifest validation
- [x] Path sanitization (`/opt/{name}-{version}/`)
- [x] Package builder structure
- [x] Deployment API design
- [x] Progress reporting through WebSocket

**Integration & Testing** ✅
- [x] Unit tests for each component
- [x] Integration tests with mock services
- [x] Runtime-agnostic validation (smol-based tests)
- [x] WebSocket client/server tests
- [x] TLS integration tests
- [x] Documentation (ADR-008 for WebSocket architecture)

**Key Design Decisions:**
- WebSocket-only API (no HTTP endpoints)
- Runtime-agnostic using async-tungstenite
- Local registry (not distributed)
- Sanitized package installation paths
- Integration with existing command-executor

### Phase 3: Networking ✅ COMPLETE

**Completed as part of orchestrator implementation**
- [x] Network module structure and types
- [x] IP allocation system
- [x] Service discovery across networks
- [x] Network topology detection (Local, LAN, WireGuard)
- [x] Cross-network service resolution
- [x] Integration tests with Docker

### Phase 4: Service Lifecycle ✅ COMPLETE

**Completed as part of orchestrator implementation**
- [x] ServiceManager implementation
- [x] Local process execution
- [x] Docker container management
- [x] Remote SSH execution
- [x] Health monitoring system
- [x] Package deployment
- [x] Dependency management
- [x] 30+ orchestrator tests

### Phase 5: Configuration System (Next - 2 weeks)

**Goal**: Enable YAML-based service definition with minimal CLI

**Week 1: Configuration Parser**
- [ ] Create harness-config crate
- [ ] Define Config, Network, Service types with serde
- [ ] YAML parser with serde_yaml
- [ ] Environment variable substitution (${VAR})
- [ ] Service reference resolution (${service.ip})
- [ ] Configuration validation
- [ ] Unit tests for parser

**Week 2: Minimal CLI**
- [ ] Create harness binary crate
- [ ] Setup clap for basic commands
- [ ] Implement commands:
  - [ ] `validate` - Validate services.yaml
  - [ ] `start` - Start service(s)
  - [ ] `stop` - Stop service(s)
  - [ ] `status` - Show service status
- [ ] Config file discovery
- [ ] Integration with orchestrator
- [ ] End-to-end tests
- [ ] Update README with YAML examples

**Deliverables**:
- Working YAML configuration
- Basic CLI that replaces Rust API usage
- Users can define and run services without writing code

### Phase 6: Full CLI Experience (2 weeks)

**Goal**: Production-ready CLI with excellent user experience

**Week 1: Core Commands**
- [ ] Service management:
  - [ ] `restart` - Restart services
  - [ ] `logs` - View service logs
  - [ ] `exec` - Execute commands in services
  - [ ] `health` - Check service health
- [ ] Configuration:
  - [ ] `init` - Initialize new project
  - [ ] `config show/env/resolve/deps/check`
- [ ] `list` - List services with filtering
- [ ] Progress indicators (indicatif)
- [ ] Table formatting (comfy-table)

**Week 2: Advanced Features & Polish**
- [ ] Network commands:
  - [ ] `network list/show/discover/test`
- [ ] Deployment:
  - [ ] `deploy` - Deploy packages
  - [ ] `package` - Create packages
- [ ] Interactive prompts for failures
- [ ] Error handling with suggestions
- [ ] Shell completions (bash/zsh/fish)
- [ ] Man page generation
- [ ] Performance optimization (<100ms startup)
- [ ] User guide documentation

**Deliverables**:
- Feature-complete CLI
- Excellent error messages and UX
- Ready for v0.1.0 release

### Phase 7: Production Features (Future)

**Goal**: Enterprise-ready features based on user feedback

**Planned Features**:
- [ ] Service auto-scaling
- [ ] Monitoring integration:
  - [ ] Prometheus metrics export
  - [ ] Grafana dashboards
  - [ ] OpenTelemetry tracing
- [ ] Advanced deployment:
  - [ ] Rolling updates
  - [ ] Blue-green deployments
  - [ ] Canary releases
- [ ] Network resilience:
  - [ ] Automatic reconnection
  - [ ] Network state recovery
  - [ ] Connection pooling
- [ ] Web UI dashboard
- [ ] Multi-region support
- [ ] Service mesh capabilities

**Success Metrics**:
- Scales to 100+ services
- Production deployments
- Enterprise adoption