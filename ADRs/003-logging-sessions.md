# ADR-003: Session-based Logging Architecture

## Status

Accepted

## Context

With the Docker-in-Docker isolation strategy, each test run generates significant logging output from multiple services. The logging system needed to:

1. **Capture all output** from docker-compose startup, image builds, test execution, and service logs
2. **Organize logs by session** to distinguish between multiple test runs
3. **Provide real-time feedback** to users during test execution
4. **Enable post-test analysis** of failures and service behavior
5. **Support concurrent test runs** without log file conflicts

Traditional approaches like redirecting to stdout/stderr or simple file logging weren't sufficient because:
- Multiple concurrent test sessions would create conflicting log files
- Service logs from within DinD containers needed to be captured and organized
- Failure analysis required structured access to service-specific logs
- Real-time streaming was needed for development feedback

## Decision

We will implement a session-based logging architecture that organizes all logs by unique session identifiers and provides both real-time streaming and persistent storage.

Key components:

### Session Management
- **Session ID Format**: `YYYY-MM-DD_HH-MM-SS_<uuid8>` for uniqueness and sortability
- **Session Directory**: Each session gets its own log directory under `test-activity/logs/sessions/`
- **Session Metadata**: JSON file tracking session status, services, and test results

### Log Organization
- **Per-Command Logs**: Each exec operation gets its own log file (e.g., `exec_cargo.log`)
- **Service Logs**: Individual log files for each Docker service when available
- **Session Timeline**: Chronological record of all session events

### Streaming Architecture
- **Real-time Output**: Commands stream to both console and log files simultaneously
- **Timestamped Entries**: All log entries include microsecond-precision timestamps
- **Async I/O**: Non-blocking log writing using tokio async I/O

Implementation in `logging.rs`:
- `LogSession` manages session lifecycle and metadata
- `LogStreamer` handles real-time log capture from Docker containers
- `LogAnalyzer` provides post-test failure analysis capabilities

## Consequences

### Positive Consequences

- **Concurrent Testing**: Multiple test sessions can run without log conflicts
- **Complete Audit Trail**: Every session captures all relevant output for debugging
- **Failure Analysis**: Structured logs enable automated failure pattern detection
- **Development Feedback**: Real-time output helps developers understand test progress
- **Historical Data**: Sessions preserved for trend analysis and regression detection

### Negative Consequences

- **Storage Usage**: Session logs can accumulate and consume significant disk space
- **I/O Overhead**: Simultaneous file and console output increases I/O load
- **Complexity**: More complex than simple stdout/stderr logging

### Risks

- **Disk Space Exhaustion**: Long-running development could fill up disk with logs
- **Log File Corruption**: Concurrent writes or crashes could corrupt log files
- **Performance Impact**: Heavy logging might slow down test execution

## Implementation

- [x] Create session-based directory structure with unique session IDs
- [x] Implement real-time log streaming to files and console
- [x] Add session metadata tracking with JSON persistence
- [x] Build async log capture for Docker container output
- [x] Create log cleanup policies to manage storage usage
- [x] Add failure analysis capabilities for post-test debugging

## Alternatives Considered

### Alternative 1: Simple File Redirection
- **Description**: Redirect all output to single log files per test run
- **Pros**: Very simple implementation, minimal overhead
- **Cons**: No real-time feedback, conflicts between concurrent runs, poor organization
- **Why rejected**: Insufficient for development workflow and concurrent testing

### Alternative 2: Centralized Logging Service
- **Description**: Send all logs to external logging service (e.g., ELK stack)
- **Pros**: Powerful search and analysis, scalable, industry standard
- **Cons**: Complex setup, external dependencies, overkill for local testing
- **Why rejected**: Too much infrastructure overhead for local development testing

### Alternative 3: Database-backed Logging
- **Description**: Store all log entries in a database with structured metadata
- **Pros**: Powerful querying, structured data, good for analysis
- **Cons**: Database dependency, more complex than files, performance concerns
- **Why rejected**: Unnecessary complexity and external dependency for local testing

### Alternative 4: In-Memory Only Logging
- **Description**: Keep all logs in memory during test run, no persistence
- **Pros**: Very fast, no disk I/O, simple cleanup
- **Cons**: Logs lost on crash, limited analysis capabilities, memory usage
- **Why rejected**: Inadequate for debugging and failure analysis needs