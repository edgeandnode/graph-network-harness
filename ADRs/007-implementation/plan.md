# ADR-007 Implementation Plan

## Overview
This document tracks the implementation of ADR-007: Distributed Service Orchestration.

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

### Phase 2: Core Infrastructure (In Progress)

Started: 2025-01-21

**Week 1: Registry Core**
- [ ] Data models (ServiceEntry, Endpoint, Location)
- [ ] In-memory store with thread-safe access
- [ ] Service CRUD operations
- [ ] State tracking and transitions
- [ ] File persistence with async-fs
- [ ] Event generation system

**Week 2: WebSocket Server** 
- [ ] async-tungstenite integration (no runtime features)
- [ ] Connection handling with async-net
- [ ] Message protocol (request/response/event)
- [ ] Event subscription management
- [ ] Connection lifecycle and error handling
- [ ] Broadcast system for events

**Week 3: Package Management**
- [ ] Package format specification
- [ ] Manifest validation
- [ ] Path sanitization (`/opt/{name}-{version}/`)
- [ ] Package builder and tarball creation
- [ ] Deployment via command-executor
- [ ] Progress reporting through WebSocket

**Integration & Testing**
- [ ] Unit tests for each component
- [ ] Integration tests with mock services
- [ ] Runtime-agnostic validation
- [ ] Example binary with different runtimes
- [ ] Documentation and API examples

**Key Design Decisions:**
- WebSocket-only API (no HTTP endpoints)
- Runtime-agnostic using async-tungstenite
- Local registry (not distributed)
- Sanitized package installation paths
- Integration with existing command-executor

### Phase 3: Networking
- [ ] WireGuard integration
- [ ] Network topology detection

### Phase 4: Service Lifecycle
- [ ] Local service management
- [ ] Remote service deployment

### Phase 5: Configuration & CLI
- [ ] Services.yaml parsing
- [ ] CLI commands

### Phase 6: Testing & Documentation
- [x] Integration tests
- [ ] Example services
- [x] **Self-hosted test orchestration**
  - [x] Use command-executor to orchestrate its own integration tests
  - [x] Replace shell scripts with Rust test harness
  - [x] Implement test container lifecycle management using library's own features
  - [x] Benefits: dogfooding, real-world usage validation, self-contained tests