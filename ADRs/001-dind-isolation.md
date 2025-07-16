# ADR-001: Docker-in-Docker Container Isolation Strategy

## Status

Accepted

## Context

The Graph Protocol's local-network stack requires comprehensive integration testing that involves multiple interdependent services (graph-node, indexer-agent, postgres, chain, etc.). Running these tests can create container name conflicts, port conflicts, and state pollution between test runs, especially when running tests in parallel or when tests fail and leave containers running.

Traditional approaches include:
1. Using a single shared Docker daemon with unique container names per test
2. Setting up separate Docker daemons per test run
3. Using container orchestration tools like Kubernetes in test mode

The challenge was to create an isolated, reproducible test environment that doesn't interfere with the developer's existing Docker setup or other concurrent test runs.

## Decision

We will use Docker-in-Docker (DinD) containers to provide complete isolation for each test session. Each test run gets its own dedicated Docker daemon running inside a DinD container, with the local-network stack deployed within this isolated environment.

Key components:
- **DinD Container**: Runs `docker:dind` image with its own Docker daemon
- **Volume Mounts**: Mount local-network source and project workspace into DinD
- **Image Synchronization**: Sync Docker images from host to DinD daemon before tests
- **Session Management**: Each test session gets a unique container name and isolated environment

## Consequences

### Positive Consequences

- **Complete Isolation**: Each test run has its own Docker daemon, eliminating container name conflicts and state pollution
- **Parallel Testing**: Multiple test sessions can run simultaneously without interference
- **Reproducible Environment**: Clean Docker state for every test run
- **Host Protection**: Test containers don't clutter the host Docker daemon
- **Network Isolation**: Each DinD container has its own Docker network namespace

### Negative Consequences

- **Performance Overhead**: DinD adds additional containerization layer and resource usage
- **Complexity**: More complex setup than directly using host Docker daemon
- **Image Storage**: Images must be synchronized from host to DinD, increasing storage requirements
- **Debugging Difficulty**: Accessing containers requires exec'ing through DinD container

### Risks

- **Resource Usage**: High memory/CPU usage when running multiple DinD instances
- **Docker Socket Security**: Requires privileged containers or Docker socket mounting
- **Image Sync Failures**: Network or storage issues could break image synchronization

## Implementation

- [x] Create DinD container configuration with docker:dind image
- [x] Implement volume mounting for local-network and workspace
- [x] Build image synchronization system using docker save/load
- [x] Add session-based container naming to prevent conflicts
- [x] Integrate with existing container management in DindManager

## Alternatives Considered

### Alternative 1: Shared Docker Daemon with Unique Names
- **Description**: Use host Docker daemon with unique container names per test run
- **Pros**: Simpler setup, better performance, easier debugging
- **Cons**: Container name conflicts, state pollution, port conflicts, difficult parallel testing
- **Why rejected**: Couldn't reliably prevent conflicts and state pollution between test runs
