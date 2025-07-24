#!/bin/bash

# Test script for daemon TLS functionality
set -e

echo "Building binaries..."
cargo build --bin harness --bin harness-executor-daemon

echo "Starting daemon in background..."
./target/debug/harness-executor-daemon &
DAEMON_PID=$!

# Give the daemon a moment to start
sleep 2

echo "Testing CLI connection..."
if ./target/debug/harness daemon status; then
    echo "✓ TLS connection successful!"
else
    echo "✗ TLS connection failed"
    kill $DAEMON_PID 2>/dev/null || true
    exit 1
fi

echo "Testing additional daemon connection..."
if ./target/debug/harness daemon status; then
    echo "✓ Second connection works!"
else
    echo "✗ Second connection failed"
fi

echo "Shutting down daemon..."
kill $DAEMON_PID 2>/dev/null || true
wait $DAEMON_PID 2>/dev/null || true

echo "✓ Test completed successfully!"