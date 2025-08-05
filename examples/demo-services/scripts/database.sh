#!/bin/sh
# Database service simulation

# Startup sequence
echo "[$(date +%H:%M:%S)] [DB] 🐘 PostgreSQL 15.3 starting..."
sleep 3
echo "[$(date +%H:%M:%S)] [DB] 📁 Initializing data directory..."
sleep 1
echo "[$(date +%H:%M:%S)] [DB] ⚙️  Loading configuration..."
echo "[$(date +%H:%M:%S)] [DB] 📊 Max connections: 100"
echo "[$(date +%H:%M:%S)] [DB] 💾 Shared buffers: 256MB"
sleep 1

# Mark as ready
touch /tmp/demo-db-ready
echo "[$(date +%H:%M:%S)] [DB] ✅ Database ready on port 5432"

# Main operation loop
while true; do
    CONNECTIONS=$((RANDOM % 50 + 10))
    echo "[$(date +%H:%M:%S)] [DB] 📊 Active connections: $CONNECTIONS/100"
    
    # Various database activities
    ACTIVITY=$((RANDOM % 8))
    case $ACTIVITY in
        0) echo "[$(date +%H:%M:%S)] [DB] ✓ Checkpoint completed (wrote 142 buffers)" ;;
        1) echo "[$(date +%H:%M:%S)] [DB] 🔧 Autovacuum: processing table 'users'" ;;
        2) 
            QUERY_TIME=$((RANDOM % 50 + 5))
            echo "[$(date +%H:%M:%S)] [DB] 🔍 Query: SELECT * FROM posts WHERE user_id = $((RANDOM % 1000)) (${QUERY_TIME}ms)"
            ;;
        3) 
            echo "[$(date +%H:%M:%S)] [WARN] [DB] ⚠️  Slow query: Complex join on feed_items (523ms)"
            ;;
        4) echo "[$(date +%H:%M:%S)] [DB] 💾 Transaction: 3 INSERTs, 1 UPDATE committed" ;;
        5) echo "[$(date +%H:%M:%S)] [DB] 📈 Index scan on posts_user_id_idx" ;;
        6) echo "[$(date +%H:%M:%S)] [DB] 🔄 Replication: Streaming to replica at 10.0.0.2" ;;
        7) 
            TABLE=$(shuf -n1 -e users posts comments likes follows)
            echo "[$(date +%H:%M:%S)] [DB] 📊 Table stats: $TABLE has $((RANDOM % 1000000 + 10000)) rows"
            ;;
    esac
    
    # Connection pool warnings
    if [ $CONNECTIONS -gt 80 ]; then
        echo "[$(date +%H:%M:%S)] [WARN] [DB] ⚠️  Connection pool usage high ($CONNECTIONS/100)"
    fi
    
    # Occasional errors
    if [ $((RANDOM % 30)) -eq 0 ]; then
        echo "[$(date +%H:%M:%S)] [ERROR] [DB] ❌ Connection refused from unauthorized IP: 192.168.1.$((RANDOM % 255))"
    fi
    
    sleep $((RANDOM % 3 + 2))
done