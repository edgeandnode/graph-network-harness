# ADR-007 Implementation Plan

## Overview
This document tracks the implementation of ADR-007: Distributed Service Orchestration.

## Implementation Phases

### Phase 0: Command Executor (In Progress)
- [x] Create command-executor crate structure
- [x] Implement core types (Executor<T>, ProcessHandle trait)
- [x] Implement LocalLauncher with basic Command/ManagedProcess support
- [x] Add tests for local command execution (8 passing tests)
- [x] Implement event streaming with line buffering (9 passing tests)
- [x] Refactor API to split event streams and process handles
- [x] **Architecture Change**: Split into Launcher/Attacher paradigms
  - [x] Create Launcher trait (for things we spawn)
  - [x] Create Attacher trait (for things we connect to)
  - [x] Implement LocalLauncher and LocalAttacher
  - [x] Split traits into separate modules
- [x] Implement ManagedService for generic service attachment
- [x] Add SystemdPortable support to LocalLauncher
- [ ] **New Architecture**: Implement nested launchers
  - [ ] Update launchers to support nesting (SshLauncher<L>)
  - [ ] Move Docker types to target module
  - [ ] LocalLauncher handles all target types
  - [ ] Implement SSH launcher with CLI wrapper
- [ ] Create comprehensive tests for nested scenarios

### Phase 1: Core Infrastructure
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