#!/bin/bash

# Run cargo check and format output for nvim quickfix
# Usage: 
#   ./nvim-warnings.sh           # Just show the warnings
#   nvim -q <(./nvim-warnings.sh) # Open nvim with quickfix populated
#   ./nvim-warnings.sh | nvim -q -  # Alternative way

cargo check --workspace --all-targets --message-format=json 2>/dev/null | \
while IFS= read -r line; do
    if command -v jq &> /dev/null; then
        echo "$line" | jq -r '
            select(.reason == "compiler-message") |
            select(.message.level == "warning" or .message.level == "error") |
            "\(.message.spans[0].file_name // "unknown"):\(.message.spans[0].line_start // 0):\(.message.spans[0].column_start // 0): \(.message.level): \(.message.message)"
        ' 2>/dev/null
    fi
done | sort -u