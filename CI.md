# CI and Testing Guide

This document describes the CI system and testing practices for the graph-network-harness project.

## Overview

The project uses a cargo xtask-based CI system that provides consistent commands for both local development and GitHub Actions. The xtask crate uses our own `command-executor` library to stream test output in real-time without buffering.

## Quick Start

### Running CI Locally

```bash
# Run all CI checks (format, clippy, tests)
cargo xtask ci all

# Run specific checks
cargo xtask ci fmt-check    # Format check (read-only)
cargo xtask ci clippy       # Linting
cargo xtask ci deny         # Dependency license and advisory check
cargo xtask ci unit-tests   # Unit tests only
cargo xtask ci integration-tests  # All integration tests
```

### Running Tests

```bash
# Run all tests
cargo xtask test --all-features

# Run tests for specific package
cargo xtask test --package service-registry

# Run tests with specific features
cargo xtask test --features docker-tests

# Run specific test by name
cargo xtask test test_name_filter
```

## Test Organization

### Test Categories

1. **Unit Tests** (`cargo xtask ci unit-tests`)
   - Fast tests that don't require external services
   - Run with `--lib --bins` flags
   - No feature flags required

2. **Integration Tests** (`cargo xtask ci integration-tests`)
   - Tests requiring Docker, SSH containers, or other services
   - Run with `--all-features` flag
   - Includes all feature-gated tests:
     - `docker-tests`: Tests requiring Docker
     - `ssh-tests`: Tests requiring SSH containers
     - `integration-tests`: Other integration tests

### Feature Flags

- `docker-tests`: Enables tests that use Docker containers
- `ssh-tests`: Enables tests that use SSH connections
- `integration-tests`: Base feature for integration tests
- `--all-features`: Runs everything (recommended for CI)

## Docker Test Images

The CI system automatically manages Docker test images:

```bash
# Build test images manually
cargo xtask docker build-test-images

# Clean test images
cargo xtask docker clean-test-images
```

Test images are:
- Built automatically when needed
- Cached in GitHub Actions using BuildKit
- Checked before each test run

Current test images:
- `command-executor-test-systemd:latest`: Ubuntu with systemd and SSH for testing

## GitHub Actions Integration

The GitHub Actions workflows use the same xtask commands:

### Workflows

1. **ci.yml**: Main CI workflow
   - Runs on push to main and pull requests
   - Jobs: fmt, clippy, deny, unit-tests, integration-tests

2. **fmt.yml**: Format checking only
   - Quick feedback on code formatting

3. **clippy.yml**: Linting only
   - Catches common Rust issues

4. **docker-test-images.yml**: Builds test containers
   - Only runs when Dockerfiles change
   - Uses GitHub Actions cache

### Test Artifacts

Test logs are automatically uploaded as artifacts:
- Named with GitHub run ID
- Retained for 7 days
- Available for download from the Actions tab

## Dependency Management

### Cargo Deny

The project uses `cargo-deny` to check for:
- License compliance
- Security advisories
- Duplicate dependencies

Configuration is in `deny.toml`. To run locally:

```bash
cargo xtask ci deny
# or directly:
cargo deny check
```

If you need to:
- Add a new allowed license: Update the `[licenses]` section
- Ignore a security advisory: Add to `[advisories]` ignore list
- Allow specific git dependencies: Update `[sources]` section

## Troubleshooting

### Common Issues

1. **Docker not available**
   - Ensure Docker daemon is running
   - Check Docker permissions

2. **Test container build fails**
   - Run `cargo xtask docker build-test-images` manually
   - Check Dockerfile syntax

3. **Integration tests timeout**
   - Check if containers are starting properly
   - Look for port conflicts

### Debugging CI Failures

1. Download test artifacts from GitHub Actions
2. Check the logs in `test-logs/` directory
3. Run the specific failing test locally:
   ```bash
   cargo xtask test --package <package> <test_name>
   ```

## Development Workflow

### Before Pushing

1. Run format check:
   ```bash
   cargo xtask ci fmt
   ```

2. Fix any formatting issues:
   ```bash
   cargo fmt --all
   ```

3. Run clippy:
   ```bash
   cargo xtask ci clippy
   ```

4. Run tests:
   ```bash
   cargo xtask ci all
   ```

### Adding New Tests

1. **Unit tests**: Add to `src/` with `#[cfg(test)]` module
2. **Integration tests**: Add to `tests/` directory
3. **Feature-gated tests**: Use appropriate `#[cfg(feature = "...")]`

### Modifying CI

The CI logic lives in the `xtask` crate:
- `xtask/src/ci.rs`: CI commands
- `xtask/src/test.rs`: Test runner
- `xtask/src/docker.rs`: Docker image management

To add new CI commands:
1. Add variant to `CiCommand` enum
2. Implement the command logic
3. Update this documentation

## Performance Notes

- The xtask system uses `command-executor` for streaming output
- No buffering - you see test output in real-time
- Docker image checks are fast (just checking existence)
- Cargo's build cache significantly speeds up repeated runs