# ADR-002: Image Synchronization from Host to DinD

## Status

Accepted

## Context

With the Docker-in-Docker (DinD) isolation strategy, each test session runs in its own Docker daemon inside a DinD container. However, this creates a problem: the DinD container starts with an empty image cache and cannot access Docker images that exist on the host system.

The Graph Protocol's local-network stack uses many Docker images:
- Base images: `postgres:17-alpine`, `ipfs/kubo:v0.34.1`, `ghcr.io/foundry-rs/foundry:v1.0.0`
- Custom images built from local Dockerfiles in the local-network directory
- Development images that may be built locally during testing

Without image synchronization, the DinD container would need to:
1. Download all base images from registries (slow, network-dependent)
2. Build all local images from scratch (very slow)
3. Handle cases where images don't exist in registries (failures)

## Decision

We will implement an automated image synchronization system that transfers Docker images from the host Docker daemon to the DinD container's Docker daemon before running tests.

The synchronization system:
1. **Analyzes docker-compose.yaml** to identify required images
2. **Checks image availability** on both host and DinD daemons
3. **Transfers missing images** using `docker save` (host) and `docker load` (DinD)
4. **Provides sync reporting** with counts of synced, skipped, and failed images
5. **Supports both registry and local images**

Key implementation in `image_sync.rs`:
- `ImageSync` struct manages the synchronization process
- `parse_compose_images()` extracts image references from docker-compose
- `sync_all()` orchestrates the entire synchronization workflow
- `ImageSyncResult` provides detailed reporting

## Consequences

### Positive Consequences

- **Fast Test Startup**: No need to download images from registries during tests
- **Offline Testing**: Tests can run without internet connectivity
- **Development Workflow**: Local image builds are immediately available in DinD
- **Bandwidth Efficiency**: Images transferred locally instead of downloaded repeatedly
- **Reliability**: No dependency on external registry availability

### Negative Consequences

- **Initial Complexity**: Image synchronization adds complexity to the startup process
- **Storage Overhead**: Images stored in both host and DinD daemon
- **Sync Time**: Large images take time to transfer (though faster than downloading)
- **Host Dependency**: DinD testing requires images to exist on host first

### Risks

- **Sync Failures**: Network or storage issues could break image transfer
- **Version Mismatches**: Host images might be different versions than expected
- **Storage Space**: Multiple DinD containers could consume significant disk space

## Implementation

- [x] Create ImageSync struct with docker-compose parsing
- [x] Implement docker save/load pipeline for image transfer
- [x] Add image existence checking to avoid unnecessary transfers
- [x] Integrate with DindManager and harness startup flow
- [x] Provide detailed sync result reporting
- [x] Handle both registry images and locally built images

## Alternatives Considered

### Alternative 1: Always Download from Registry
- **Description**: Let DinD container download all images from Docker registries
- **Pros**: Simple implementation, no sync complexity
- **Cons**: Slow startup, network dependency, doesn't work for local images
- **Why rejected**: Too slow for development workflow and breaks offline testing

### Alternative 2: Pre-built DinD Image with All Dependencies
- **Description**: Create a custom DinD image that includes all required images
- **Pros**: Fast startup, no runtime sync needed
- **Cons**: Very large image, difficult to maintain, doesn't support local development
- **Why rejected**: Inflexible for development and would create massive Docker images

### Alternative 3: Shared Docker Socket
- **Description**: Mount host Docker socket into DinD container to share image cache
- **Pros**: No image synchronization needed, shared image cache
- **Cons**: Breaks isolation principle, security concerns, potential conflicts
- **Why rejected**: Defeats the purpose of DinD isolation and creates security risks

### Alternative 4: Registry-based Sync
- **Description**: Push images to a registry and pull them in DinD
- **Pros**: Uses standard Docker workflows, supports distributed setups
- **Cons**: Requires registry setup, network dependency, complex authentication
- **Why rejected**: Too much infrastructure complexity for local testing