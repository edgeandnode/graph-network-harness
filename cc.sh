#!/bin/bash

# Run cargo check with JSON output and parse it to show warnings/errors
# Format: file:line: level: message

cargo check --workspace --all-targets --message-format=json 2>/dev/null | \
while IFS= read -r line; do
    # Parse JSON using jq if available, otherwise use a simple grep approach
    if command -v jq &> /dev/null; then
        echo "$line" | jq -r '
            select(.reason == "compiler-message") |
            select(.message.level == "warning" or .message.level == "error") |
            "\(.message.spans[0].file_name // "unknown"):\(.message.spans[0].line_start // 0): \(.message.level): \(.message.message)"
        ' 2>/dev/null
    else
        # Fallback without jq - basic parsing
        if echo "$line" | grep -q '"reason":"compiler-message"'; then
            # Extract key fields using sed (less reliable but works without jq)
            file=$(echo "$line" | sed -n 's/.*"file_name":"\([^"]*\)".*/\1/p' | head -1)
            line_num=$(echo "$line" | sed -n 's/.*"line_start":\([0-9]*\).*/\1/p' | head -1)
            level=$(echo "$line" | sed -n 's/.*"level":"\([^"]*\)".*/\1/p' | head -1)
            message=$(echo "$line" | sed -n 's/.*"message":"\([^"]*\)".*/\1/p' | head -1)
            
            if [ -n "$file" ] && [ -n "$line_num" ] && [ -n "$level" ] && [ -n "$message" ]; then
                echo "$file:$line_num: $level: $message"
            fi
        fi
    fi
done | sort -u  # Remove duplicates and sort by filename