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

### Phase 2: Core Infrastructure
- [ ] Create service registry module
  - [ ] Service registration/deregistration
  - [ ] IP address tracking (host, LAN, WireGuard)
  - [ ] Service dependency tracking
  - [ ] Registry persistence

- [ ] Integrate command-executor with service registry
  - [x] Local execution via LocalLauncher
  - [x] Docker support via target types
  - [x] Remote execution via nested launchers

- [ ] Build package management
  - [ ] Package creation from manifest
  - [ ] Tarball generation
  - [ ] Script validation
  - [ ] Package extraction

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