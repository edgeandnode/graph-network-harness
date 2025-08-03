# Architectural Decision Records (ADRs)

This directory contains the architectural decision records for the graph-network-harness project. ADRs document significant architectural and design decisions made during development.

## Index

| ADR | Title | Status | Description |
|-----|-------|--------|-------------|
| [001](001-dind-isolation.md) | Docker-in-Docker Container Isolation Strategy | Accepted | Isolate test environments using DinD |
| [002](002-image-sync.md) | Image Synchronization from Host to DinD | Accepted | Sync Docker images between host and DinD |
| [003](003-logging-sessions.md) | Session-based Logging Architecture | Accepted | Persistent logging with session management |
| [004](004-project-structure.md) | Project Structure and Binary Naming | Accepted | Workspace organization and naming conventions |
| [005](005-service-events.md) | Service Event Streaming and Inspection API | Accepted | Real-time service event streaming |
| [006](006-container-paths.md) | Container Path Management | Accepted | Path handling in containerized environments |
| [007](007-distributed-service-orchestration.md) | Distributed Service Orchestration | Accepted (80% Complete) | Heterogeneous service orchestration across environments |
| [008](008-websocket-architecture.md) | WebSocket-Based Service Communication | Accepted | WebSocket API for service management |
| [009](009-network-testing-strategy.md) | Network Testing Strategy | Accepted | Testing approach for network configurations |
| [010](010-domain-oriented-action-system.md) | Domain-Oriented Action System | Accepted | Type-safe action/event system for services |
| [011](011-graph-test-daemon.md) | Graph Test Daemon Design | Accepted | Specialized daemon for Graph Protocol testing |
| [012](012-service-registry-sled-backend.md) | Service Registry Sled Backend | Accepted | Persistent storage using sled database |

## ADR Lifecycle

- **Proposed**: Under discussion and review
- **Accepted**: Decision made and approved for implementation
- **Deprecated**: No longer the preferred approach but retained for history
- **Superseded**: Replaced by a newer ADR

## Template

Use the [template](template.md) when creating new ADRs to ensure consistency.

## About ADRs

Architectural Decision Records (ADRs) are short text documents that capture important architectural decisions along with their context and consequences. They help future developers understand why certain choices were made.