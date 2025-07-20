#!/bin/bash
# Script to create test portable service images

set -e

cd "$(dirname "$0")"

# Create a simple echo service
echo "Creating echo-service portable image..."
mkdir -p test-services/echo-service/{usr/bin,etc/systemd/system}

# Create the service executable
cat > test-services/echo-service/usr/bin/echo-service.sh << 'EOF'
#!/bin/bash
echo "Echo service started at $(date)"
while true; do
    echo "Echo service running at $(date)"
    sleep 5
done
EOF
chmod +x test-services/echo-service/usr/bin/echo-service.sh

# Create the systemd unit file
cat > test-services/echo-service/etc/systemd/system/echo-service.service << 'EOF'
[Unit]
Description=Test Echo Service
After=multi-user.target

[Service]
Type=simple
ExecStart=/usr/bin/echo-service.sh
Restart=always
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

# Create os-release for portable service
cat > test-services/echo-service/etc/os-release << 'EOF'
ID=echo-service
VERSION_ID=1.0
PORTABLE_PREFIXES=echo-service
EOF

# Create a counter service that writes to a file
echo "Creating counter-service portable image..."
mkdir -p test-services/counter-service/{usr/bin,etc/systemd/system,var/lib}

# Create the service executable
cat > test-services/counter-service/usr/bin/counter-service.sh << 'EOF'
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
EOF
chmod +x test-services/counter-service/usr/bin/counter-service.sh

# Create the systemd unit file
cat > test-services/counter-service/etc/systemd/system/counter-service.service << 'EOF'
[Unit]
Description=Test Counter Service
After=multi-user.target

[Service]
Type=simple
ExecStart=/usr/bin/counter-service.sh
Restart=always
StandardOutput=journal
StandardError=journal
# Allow writing to /var/lib
ReadWritePaths=/var/lib/counter-service

[Install]
WantedBy=multi-user.target
EOF

# Create os-release for portable service
cat > test-services/counter-service/etc/os-release << 'EOF'
ID=counter-service
VERSION_ID=1.0
PORTABLE_PREFIXES=counter-service
EOF

echo "Test portable services created in test-services/"
echo ""
echo "To create portable images, run inside the container:"
echo "  cd /opt/portable-services"
echo "  tar -czf echo-service.tar.gz -C echo-service ."
echo "  tar -czf counter-service.tar.gz -C counter-service ."