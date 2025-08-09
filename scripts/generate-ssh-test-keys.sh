#!/bin/bash
# Generate SSH keys for integration tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Find the workspace root
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
WORKSPACE_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

# SSH keys directory
SSH_KEYS_DIR="$WORKSPACE_ROOT/crates/command-executor/tests/systemd-container/ssh-keys"

echo -e "${YELLOW}Generating SSH test keys...${NC}"

# Create directory if it doesn't exist
mkdir -p "$SSH_KEYS_DIR"

# Generate ED25519 key if it doesn't exist
if [ ! -f "$SSH_KEYS_DIR/test_ed25519" ]; then
    echo -e "${GREEN}Generating new ED25519 key...${NC}"
    ssh-keygen -t ed25519 \
        -f "$SSH_KEYS_DIR/test_ed25519" \
        -N "" \
        -C "test@graph-network-harness" \
        -q
    
    # Create authorized_keys file
    cp "$SSH_KEYS_DIR/test_ed25519.pub" "$SSH_KEYS_DIR/authorized_keys"
    
    # Set proper permissions
    chmod 600 "$SSH_KEYS_DIR/test_ed25519"
    chmod 644 "$SSH_KEYS_DIR/test_ed25519.pub"
    chmod 644 "$SSH_KEYS_DIR/authorized_keys"
    
    echo -e "${GREEN}✓ SSH test keys generated successfully${NC}"
    echo -e "  Private key: $SSH_KEYS_DIR/test_ed25519"
    echo -e "  Public key:  $SSH_KEYS_DIR/test_ed25519.pub"
    echo -e "  Authorized:  $SSH_KEYS_DIR/authorized_keys"
else
    echo -e "${YELLOW}SSH test keys already exist${NC}"
    
    # Ensure authorized_keys is up to date
    if [ ! -f "$SSH_KEYS_DIR/authorized_keys" ] || [ "$SSH_KEYS_DIR/test_ed25519.pub" -nt "$SSH_KEYS_DIR/authorized_keys" ]; then
        echo -e "${GREEN}Updating authorized_keys...${NC}"
        cp "$SSH_KEYS_DIR/test_ed25519.pub" "$SSH_KEYS_DIR/authorized_keys"
        chmod 644 "$SSH_KEYS_DIR/authorized_keys"
    fi
fi

echo
echo -e "${GREEN}SSH test keys are ready!${NC}"
echo
echo "To run SSH integration tests:"
echo "  cargo test -p command-executor --features ssh-tests,docker-tests"
echo "  cargo test -p service-orchestration --features ssh-tests,docker-tests"