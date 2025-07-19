# Local Network Harness - Architectural Decision Records (ADRs)

This directory contains the architectural decision records for the local-network-harness project. ADRs document significant architectural and design decisions made during the development of this project.

## Index

| ADR | Title | Status | Date |
|-----|-------|--------|------|
| [001](001-dind-isolation.md) | Docker-in-Docker Container Isolation Strategy | Accepted | 2025-07-16 |
| [002](002-image-sync.md) | Image Synchronization from Host to DinD | Accepted | 2025-07-16 |
| [003](003-logging-sessions.md) | Session-based Logging Architecture | Accepted | 2025-07-16 |
| [004](004-project-structure.md) | Project Structure and Binary Naming | Accepted | 2025-07-16 |
| [005](005-service-events.md) | Service Event Streaming and Inspection API | Proposed | 2025-07-16 |
| [006](006-container-paths.md) | Container Path Management | Accepted | 2025-07-16 |
| [007](007-distributed-service-orchestration.md) | Distributed Service Orchestration | Proposed | 2025-07-18 |
| [012](012-yaml-service-registry.md) | YAML-Based Service Registry | Proposed | 2025-07-17 |
| [013](013-project-rename-workspace.md) | Project Rename and Workspace Structure | Proposed | 2025-07-17 |

## ADR Lifecycle

- **Proposed**: Under discussion and review
- **Accepted**: Decision made and approved for implementation
- **Deprecated**: No longer the preferred approach but not removed for historical reference
- **Superseded**: Replaced by a newer ADR

## Template

Use the [template](template.md) when creating new ADRs to ensure consistency.

## About ADRs

Architectural Decision Records (ADRs) are short text documents that capture an important architectural decision made along with its context and consequences. For more information about ADRs, see [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions).