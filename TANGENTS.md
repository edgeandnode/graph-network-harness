# Tangents and Issues

Template:
- [ ] Brief description of the issue
  - Steps: `command to reproduce` or brief steps
  - Error: Key error message if applicable

## Service Failures
- [ ] tap-contracts container fails to start
  - Steps: `cargo run --bin stack-runner -- exec --local-network ../submodules/local-network -- docker logs tap-contracts`
  - Error: `Flag --version-label can only be specified once`

- [ ] block-oracle container fails to start
  - Steps: `cargo run --bin stack-runner -- exec --local-network ../submodules/local-network -- docker logs block-oracle`
  - Error: `This project is configured to use pnpm`

## Performance
- [ ] Docker image sync takes minutes even when images exist
  - Steps: Run service_inspection example twice, note "Already exists" but still takes time
  - Fix: Consider caching checksums or using docker inspect

## Code Cleanup
- [ ] Multiple unused imports and constants throughout codebase
  - Steps: `cargo build 2>&1 | grep warning | wc -l` shows 17 warnings
  - Fix: Run cargo fix or manually clean up

## Missing Examples
- [ ] No example showing real-time event streaming
  - Steps: Check examples/, no use of `inspector.start_streaming(containers)`
  - Fix: Create streaming_example.rs

- [ ] No custom ServiceEventHandler implementation example
  - Steps: Only built-in handlers (postgres, graph-node, generic)
  - Fix: Create custom_handler_example.rs