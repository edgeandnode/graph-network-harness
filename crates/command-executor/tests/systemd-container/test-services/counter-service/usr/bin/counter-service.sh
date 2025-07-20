#!/bin/bash
COUNTER_FILE=/var/lib/counter-service/counter
mkdir -p /var/lib/counter-service

if [ -f "$COUNTER_FILE" ]; then
    COUNTER=$(cat "$COUNTER_FILE")
else
    COUNTER=0
fi

echo "Counter service started with value: $COUNTER"

while true; do
    COUNTER=$((COUNTER + 1))
    echo "$COUNTER" > "$COUNTER_FILE"
    echo "Counter: $COUNTER"
    sleep 2
done
