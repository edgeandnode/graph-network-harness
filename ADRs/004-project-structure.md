# ADR-004: Project Structure and Binary Naming

## Status

Accepted

## Context

The original project was named `integration-tests` and structured as a simple binary crate for testing the indexer-agent. However, as development progressed, several issues became apparent:

1. **Generic Purpose**: The functionality was useful for any component that needed to test against the Graph Protocol local-network stack, not just the indexer-agent
2. **Library Potential**: Other projects could benefit from the Docker-in-Docker harness functionality
3. **Misleading Name**: "integration-tests" suggested it only contained tests, when it actually provided a reusable testing harness
4. **Binary Naming**: The binary name matched the crate name, which wasn't descriptive of its function

The project needed restructuring to:
- Reflect its broader purpose as a testing harness for Graph Protocol components
- Provide both library and binary interfaces
- Use more descriptive and professional naming

## Decision

We will restructure the project with a more appropriate name and clear separation between library and binary functionality:

### Project Rename
- **From**: `integration-tests`
- **To**: `local-network-harness`
- **Rationale**: Clearly indicates it's a harness/toolkit for working with the local-network stack

### Binary Rename
- **From**: `integration-tests` (same as crate)
- **To**: `stack-runner`
- **Rationale**: Describes what the binary does - runs the Graph Protocol stack

### Project Structure
```
local-network-harness/
├── Cargo.toml                 # Package: local-network-harness
├── src/
│   ├── lib.rs                 # Library interface
│   ├── main.rs                # Binary: stack-runner
│   ├── harness/               # Core harness functionality
│   ├── container/             # Docker management
│   └── logging/               # Session logging
└── README.md                  # Updated documentation
```

### Configuration
```toml
[package]
name = "local-network-harness"

[lib]
name = "local_network_harness"
path = "src/lib.rs"

[[bin]]
name = "stack-runner"
path = "src/main.rs"
```

## Consequences

### Positive Consequences

- **Clear Purpose**: Name clearly indicates this is a harness for local-network testing
- **Reusable Library**: Other projects can use `local_network_harness` as a dependency
- **Professional Naming**: More appropriate for potential standalone distribution
- **Flexible Usage**: Can be used as both library and CLI tool
- **Better Documentation**: README and docs can focus on the harness capabilities

### Negative Consequences

- **Breaking Change**: Existing references to `integration-tests` need updating
- **Import Changes**: Code using the library needs to update import paths
- **Git History**: Some confusion in git history during transition period

### Risks

- **Missed References**: Some hardcoded references to old names might be missed
- **Documentation Lag**: Documentation might temporarily be inconsistent during transition

## Implementation

- [x] Update Cargo.toml with new package and binary names
- [x] Rename project directory from integration-tests to local-network-harness
- [x] Update workspace Cargo.toml to reference new name
- [x] Fix all import statements from `integration_tests::` to `local_network_harness::`
- [x] Update README.md with new project name and binary references
- [x] Update documentation to use `target/debug/stack-runner` instead of cargo commands

## Alternatives Considered

### Alternative 1: Keep Original Name
- **Description**: Continue using `integration-tests` as the project name
- **Pros**: No breaking changes, consistent with existing documentation
- **Cons**: Misleading name, not suitable for library distribution, unclear purpose
- **Why rejected**: Name doesn't reflect the broader utility of the harness

### Alternative 2: graph-local-harness
- **Description**: Name the project `graph-local-harness` to include "graph" branding
- **Pros**: Clear Graph Protocol association, professional naming
- **Cons**: Too close to official Graph CLI tools, might create confusion
- **Why rejected**: Could be confused with official Graph tooling

### Alternative 3: integration-harness
- **Description**: Rename to `integration-harness` to keep "integration" concept
- **Pros**: Maintains connection to testing, clear harness designation
- **Cons**: Still focused on "integration" rather than the local-network stack specifically
- **Why rejected**: Less specific than `local-network-harness` about what it does

### Alternative 4: Keep Binary Name Same as Crate
- **Description**: Continue using the same name for both crate and binary
- **Pros**: Simpler configuration, consistent naming
- **Cons**: Binary name `local-network-harness` is not very descriptive of functionality
- **Why rejected**: `stack-runner` is more descriptive of what the binary actually does