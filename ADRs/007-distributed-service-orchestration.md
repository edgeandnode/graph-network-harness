# ADR-007: Distributed Service Orchestration

## Status
Accepted - Implementation 80% Complete

## Context

Traditional Docker-based test harnesses have significant limitations when used for integration testing and development. Container rebuilds are slow (30-60 seconds for small changes), debugging through container boundaries is complex, and Docker's bridge network doesn't extend to native processes or remote machines. Additionally, running everything in containers adds unnecessary resource overhead and makes it difficult to test distributed scenarios.

## Decision

Transform the test harness from a Docker-only system into a **heterogeneous service harness** that can run services anywhere (local processes, containers, remote machines) while making them all appear to be on the same network.

### Core Features

1. **Run Anywhere**: Services can execute as:
   - Local processes (for fast dev iteration)
   - Docker containers (for isolation)
   - LAN nodes requiring remote administration but no vpn is needed to reach them.
   - Remote processes via SSH (for distributed testing)

2. **Unified Networking**: All services can talk to each other regardless of where/how they run:
   - Direct LAN connections
   - Local processes
   - Local and remote docker containers
   - WireGuard mesh creates a virtual network spanning all locations

3. **Transparent Discovery**: Services find each other by name:
   - Automatically routes to correct IP based on caller location

4. **Zero-Config Deployment**: Package and deploy to remote machines:
   - Bundle service + config + setup script
   - Deploy via SSH with automatic network setup

## Implementation Summary

### Architecture Overview

**Service Execution Targets**
- Local Process: Direct process execution for fast development iteration
- Docker Container: Container-based execution for isolation
- Remote LAN: Services on LAN-accessible nodes without VPN requirements
- WireGuard Remote: Services on remote nodes with secure mesh networking

**Network Topology**
- Automatic Network Selection: System analyzes service locations and automatically determines networking strategy
- Direct LAN Connectivity: Services on same network communicate directly
- WireGuard Mesh: Created on-demand only when remote nodes require secure tunneling
- IP-Based Discovery: Services use direct IP addresses injected via environment variables - no DNS complexity

**Key Design Decisions**
- Nested Launcher Architecture: Composable launcher pattern allows combining execution contexts
- Runtime Agnostic: All components work with any async runtime
- Event Streaming: Unified event system for process output, state changes, and health monitoring
- Package-Based Deployment: Services deployed to remote nodes as self-contained tarballs

### Implementation Phases

1. **Command Executor** ✅ Complete
   - Runtime-agnostic command execution across backends
   - Launcher/Attacher pattern for spawning vs connecting
   - Nested launcher architecture for composable contexts

2. **Service Registry** ✅ Complete
   - WebSocket-based service discovery
   - Thread-safe state management with persistence
   - Event generation and subscription system

3. **Networking** ✅ Complete
   - Network topology detection and IP allocation
   - Cross-network service resolution
   - Automatic WireGuard mesh creation when needed

4. **Service Lifecycle** ✅ Complete
   - ServiceManager orchestration with dependency resolution
   - Multi-backend execution support
   - Health monitoring and event streaming

5. **Configuration System** 🚧 In Progress (2 weeks)
   - YAML configuration parser
   - Basic CLI commands (validate, start, stop, status)
   - Environment variable and service reference substitution

6. **Full CLI Experience** 📋 Planned (2 weeks)
   - Interactive features and enhanced UX
   - Advanced commands and monitoring

7. **Production Features** 🔮 Future
   - Auto-scaling, monitoring integration, web UI

## Consequences

### Positive
- **10x faster development iteration** with native process execution
- **Unified testing** across local, containerized, and distributed environments
- **Simplified debugging** with direct process access
- **Resource efficiency** by choosing appropriate runtime per service
- **Network transparency** with automatic service discovery

### Negative
- **Increased complexity** in the harness implementation
- **Platform limitations** - full features only on Linux
- **Security considerations** for SSH keys and WireGuard configuration
- **Learning curve** for new runtime options

### Mitigations
- Sensible defaults that "just work"
- Automatic feature detection and graceful degradation
- Comprehensive documentation and examples
- Security best practices built-in

## Current Status

The core orchestration engine is 80% complete. All fundamental features are implemented and tested. The remaining work (Phase 5) focuses on making the system accessible through YAML configuration and CLI commands rather than requiring Rust code. This final piece will transform the current library implementation into a complete tool that fulfills the ADR's vision.

## References

- [WireGuard](https://www.wireguard.com/) - Fast, modern VPN protocol
- See ADRs 008-012 for related architectural decisions