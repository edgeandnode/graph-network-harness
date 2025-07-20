#!/bin/bash
# Generate SSH keys for test container

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SSH_DIR="$SCRIPT_DIR/ssh-keys"

echo "Generating SSH keys for test container..."

# Create ssh-keys directory
mkdir -p "$SSH_DIR"
chmod 700 "$SSH_DIR"

# Generate RSA key (for compatibility)
if [ ! -f "$SSH_DIR/test_rsa" ]; then
    ssh-keygen -t rsa -b 4096 -f "$SSH_DIR/test_rsa" -N "" -C "command-executor-test"
    echo "Generated RSA key: $SSH_DIR/test_rsa"
else
    echo "RSA key already exists: $SSH_DIR/test_rsa"
fi

# Generate ED25519 key (modern, faster)
if [ ! -f "$SSH_DIR/test_ed25519" ]; then
    ssh-keygen -t ed25519 -f "$SSH_DIR/test_ed25519" -N "" -C "command-executor-test"
    echo "Generated ED25519 key: $SSH_DIR/test_ed25519"
else
    echo "ED25519 key already exists: $SSH_DIR/test_ed25519"
fi

# Create authorized_keys file
cat "$SSH_DIR/test_rsa.pub" "$SSH_DIR/test_ed25519.pub" > "$SSH_DIR/authorized_keys"
chmod 600 "$SSH_DIR/authorized_keys"

echo ""
echo "SSH keys generated successfully!"
echo ""
echo "Keys location: $SSH_DIR"
echo "- RSA key: test_rsa"
echo "- ED25519 key: test_ed25519"
echo ""
echo "To use these keys in tests:"
echo "  ssh -i $SSH_DIR/test_ed25519 -p 2223 testuser@localhost"
echo ""
echo "Note: These keys are for testing only and are gitignored."