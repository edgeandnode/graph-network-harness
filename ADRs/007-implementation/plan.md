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

### Phase 3: Networking (Next Phase)

**Core Philosophy**: Test-driven development with docker-compose simulating network topologies

**Week 1: Network Foundation & Basic Tests**
- [ ] Network module structure and types
- [ ] IP allocation system with tests
- [ ] Docker-compose test infrastructure setup
- [ ] Basic local connectivity tests

**Week 2: LAN Discovery & Testing**
- [ ] LAN service discovery implementation
- [ ] Docker-compose multi-network tests simulating LAN
- [ ] Service IP resolution logic
- [ ] Integration tests for mixed local/LAN scenarios

**Week 3: WireGuard Integration & Advanced Tests**
- [ ] WireGuard configuration generation
- [ ] Mock WireGuard for testing (no root required)
- [ ] Full topology detection with tests
- [ ] End-to-end tests with all network types

**Testing Infrastructure**
- [ ] Docker-compose configurations for network scenarios
- [ ] Test harness for network topology validation
- [ ] Performance tests for network path selection
- [ ] Failure scenario tests (network partitions, etc.)

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