#!/bin/sh
# Cache service simulation (Redis-like)

echo "[$(date +%H:%M:%S)] [CACHE] 🔴 Redis 7.0.5 starting..."
sleep 1
echo "[$(date +%H:%M:%S)] [CACHE] 💾 Loading data from disk..."
echo "[$(date +%H:%M:%S)] [CACHE] 🔧 Max memory: 2GB"
echo "[$(date +%H:%M:%S)] [CACHE] 📊 Eviction policy: allkeys-lru"

# Start a dummy listener for health checks
nc -l -k -p 6379 2>/dev/null &
echo "[$(date +%H:%M:%S)] [CACHE] ✅ Ready to accept connections on port 6379"

# Main loop
TOTAL_KEYS=0
HITS=0
MISSES=0

while true; do
    # Simulate cache operations
    OP=$((RANDOM % 10))
    
    if [ $OP -lt 7 ]; then
        # Cache hit
        HITS=$((HITS + 1))
        KEY=$(shuf -n1 -e user:$((RANDOM % 1000)) post:$((RANDOM % 5000)) feed:$((RANDOM % 100)))
        echo "[$(date +%H:%M:%S)] [CACHE] ✓ GET $KEY (hit) - 0.$(printf '%03d' $((RANDOM % 100)))ms"
    elif [ $OP -lt 9 ]; then
        # Cache miss
        MISSES=$((MISSES + 1))
        KEY="session:$((RANDOM % 10000))"
        echo "[$(date +%H:%M:%S)] [CACHE] ✗ GET $KEY (miss) - 0.$(printf '%03d' $((RANDOM % 100)))ms"
    else
        # Set operation
        TOTAL_KEYS=$((TOTAL_KEYS + 1))
        KEY=$(shuf -n1 -e user post comment notification)
        echo "[$(date +%H:%M:%S)] [CACHE] 💾 SET $KEY:$((RANDOM % 5000)) - TTL 3600s"
    fi
    
    # Periodic stats
    if [ $((RANDOM % 20)) -eq 0 ]; then
        HIT_RATE=0
        if [ $((HITS + MISSES)) -gt 0 ]; then
            HIT_RATE=$(( HITS * 100 / (HITS + MISSES) ))
        fi
        echo "[$(date +%H:%M:%S)] [CACHE] 📊 Stats: Keys=$TOTAL_KEYS, Hit Rate=$HIT_RATE%, Memory=234MB/2048MB"
    fi
    
    # Eviction messages
    if [ $((RANDOM % 30)) -eq 0 ]; then
        echo "[$(date +%H:%M:%S)] [CACHE] 🗑️  Evicted 127 keys (LRU)"
    fi
    
    sleep 0.$((RANDOM % 5 + 1))
done