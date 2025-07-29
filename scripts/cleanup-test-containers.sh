#!/bin/bash
# Cleanup script for harness test containers

echo "Cleaning up harness test containers..."

# Find and remove all containers with -harness-test suffix
containers=$(docker ps -a -q -f name=.*-harness-test)

if [ -n "$containers" ]; then
    echo "Found test containers:"
    docker ps -a -f name=.*-harness-test --format "table {{.Names}}\t{{.Status}}"
    echo ""
    echo "Removing containers..."
    docker rm -f $containers
    echo "Cleanup complete!"
else
    echo "No harness test containers found."
fi