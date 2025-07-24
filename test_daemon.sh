#!/bin/bash
# Test script for daemon and CLI communication

echo "Starting daemon in background..."
cargo run --bin harness-executor-daemon &
DAEMON_PID=$!

# Wait for daemon to start
sleep 2

echo -e "\nTesting status command..."
cargo run --bin harness -- status

echo -e "\nKilling daemon..."
kill $DAEMON_PID 2>/dev/null

echo "Test complete"