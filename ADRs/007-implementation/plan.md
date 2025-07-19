# ADR-007 Implementation Plan

## Overview
This document tracks the implementation of ADR-007: Distributed Service Orchestration.

## Implementation Phases

### Phase 0: Command Executor (In Progress)
- [x] Create command-executor crate structure
- [x] Implement core types (Executor<T>, Backend trait, Process trait)
- [x] Implement LocalBackend with basic Command/ManagedProcess support
- [x] Add tests for local command execution (8 passing tests)
- [x] Implement event streaming with line buffering (9 passing tests)
- [ ] Add SystemdService support to LocalBackend
- [ ] Add SystemdPortable support to LocalBackend
- [ ] Implement SSH backend
- [ ] Implement Docker backend (pending runtime-agnostic solution)

### Phase 1: Core Infrastructure
- [ ] Create service registry module
  - [ ] Service registration/deregistration
  - [ ] IP address tracking (host, LAN, WireGuard)
  - [ ] Service dependency tracking
  - [ ] Registry persistence

- [ ] Integrate command-executor with service registry
  - [x] Process runtime trait (Backend trait in command-executor)
  - [ ] Docker runtime (container execution)
  - [ ] Remote runtime (SSH execution)

- [ ] Build package management
  - [ ] Package creation from manifest
  - [ ] Tarball generation
  - [ ] Script validation
  - [ ] Package extraction

### Phase 2: Networking
- [ ] WireGuard integration
- [ ] Network topology detection

### Phase 3: Service Lifecycle
- [ ] Local service management
- [ ] Remote service deployment

### Phase 4: Configuration & CLI
- [ ] Services.yaml parsing
- [ ] CLI commands

### Phase 5: Testing & Documentation
- [ ] Integration tests
- [ ] Example services