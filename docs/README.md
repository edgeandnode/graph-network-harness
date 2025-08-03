# Documentation Index

## Overview Documents

- **[ARCHITECTURE.md](./ARCHITECTURE.md)** - Complete system architecture, component relationships, and design principles
- **[CURRENT-STATUS.md](./CURRENT-STATUS.md)** - Implementation progress, component completion status, known issues
- **[DEVELOPER-GUIDE.md](./DEVELOPER-GUIDE.md)** - Setup instructions, common development tasks, debugging tips

## Reference Documents

- **[CODE-POLICY.md](./CODE-POLICY.md)** - Coding standards, architectural principles, and best practices
- **[service-orchestration-architecture.md](./service-orchestration-architecture.md)** - Deep dive into service orchestration and daemon integration
- **[ISSUES-BACKLOG.md](./ISSUES-BACKLOG.md)** - Known issues and future improvements

## Architecture Decision Records

See the [ADRs directory](../ADRs/) for detailed architectural decisions:

- **[ADR-007](../ADRs/007-distributed-service-orchestration.md)** - Core orchestration design
- **[ADR-010](../ADRs/010-domain-oriented-action-system.md)** - Service and action model
- **[ADR-011](../ADRs/011-graph-test-daemon.md)** - Graph Protocol testing design

## Component Documentation

Each crate should have its own README with specific details:

- `command-executor` - Command execution and layering system
- `service-registry` - Service discovery and network management  
- `service-orchestration` - Service lifecycle and health monitoring
- `harness-config` - YAML configuration format
- `harness-core` - Core abstractions and traits
- `harness` - CLI usage and daemon protocol
- `graph-test-daemon` - Graph Protocol testing services

## Quick Links

### For Users
- [Quick Start](../README.md#quick-start) - Get up and running quickly
- [Graph Protocol Testing](../README.md#graph-protocol-testing) - Using graph-test-daemon

### For Contributors  
- [Development Setup](./DEVELOPER-GUIDE.md#development-setup) - Environment configuration
- [Common Tasks](./DEVELOPER-GUIDE.md#common-tasks) - How to extend the system
- [Testing](./DEVELOPER-GUIDE.md#testing) - Test infrastructure and patterns
- [CI/CD](../CI.md) - Continuous integration setup

### For Architects
- [Design Principles](./ARCHITECTURE.md#design-principles) - Core architectural guidelines
- [Extension Points](./ARCHITECTURE.md#extension-points) - How to add new capabilities
- [Future Enhancements](./ARCHITECTURE.md#future-enhancements) - Planned improvements