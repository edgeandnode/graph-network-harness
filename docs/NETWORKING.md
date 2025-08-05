# Networking Guide

The harness supports orchestrating services across different network environments:

## Supported Network Types

### Local
Services running on the same machine as the harness.
- **Use case**: Development, single-machine deployments
- **Configuration**: No special network setup required

### LAN (Local Area Network)
Services running on other machines within the same network segment.
- **Use case**: Multi-machine deployments within a data center or office
- **Configuration**: Services must be reachable via their LAN IP addresses

### Remote (SSH)
Services running on remote machines accessible via SSH.
- **Use case**: Distributed deployments, cloud instances, cross-network services
- **Configuration**: SSH key authentication required

## WireGuard Integration (Manual Setup)

**Note**: Built-in WireGuard management has been deprecated. Users should configure WireGuard manually.

### Recommended Approach

1. **Set up WireGuard mesh network** using standard tools:
   ```bash
   # Example using wg-quick
   sudo wg-quick up wg0
   ```

2. **Configure services to use WireGuard IPs**:
   ```yaml
   services:
     remote-service:
       target:
         type: remote-ssh
         host: "10.42.0.10"  # WireGuard IP
         user: "appuser"
         binary: "myapp"
         args: []
         env: {}
   ```

3. **Benefits**:
   - Full control over WireGuard configuration
   - Integration with existing VPN infrastructure
   - No additional privileges required for harness
   - Works with any VPN solution (not just WireGuard)

### Alternative VPN Solutions

The harness works with any VPN that provides routable IP addresses:
- **Tailscale**: Zero-config WireGuard mesh
- **OpenVPN**: Traditional VPN solution  
- **Cloud VPNs**: AWS VPC, GCP VPN, Azure VPN
- **Manual WireGuard**: Full control setup

## Configuration Examples

### Local Development
```yaml
services:
  database:
    target:
      Local:
        binary: "postgres"
        args: ["-D", "/var/lib/postgresql/data"]
```

### LAN Deployment
```yaml
services:
  api-server:
    target:
      type: remote-ssh
      host: "192.168.1.100"
      user: "deploy"
      binary: "api-server"
      args: []
      env: {}
```

### WireGuard Mesh (Manual Setup)
```yaml
# After setting up WireGuard with wg-quick or similar
services:
  secure-service:
    target:
      type: remote-ssh
      host: "10.42.0.5"  # WireGuard IP
      user: "secure"
      binary: "secure-service"
      args: []
      env: {}
```

## Migration from Built-in WireGuard

If you were using the deprecated `Wireguard` target type:

1. Set up WireGuard manually on all nodes
2. Replace `Wireguard` targets with `Remote` targets using WireGuard IPs
3. Remove WireGuard-specific configuration from harness config

See [WIREGUARD-DEPRECATION.md](WIREGUARD-DEPRECATION.md) for detailed migration instructions.

## Security Considerations

- **SSH Keys**: Use key-based authentication for Remote targets
- **Network Isolation**: Use VPN/WireGuard for secure communication
- **Firewall Rules**: Restrict access to required ports only
- **Service Discovery**: Consider using private/internal IPs where possible

## Troubleshooting

### SSH Connection Issues
```bash
# Test SSH connectivity
ssh user@host "echo 'Connection successful'"

# Check SSH key authentication
ssh -i ~/.ssh/id_rsa user@host
```

### WireGuard Connectivity
```bash
# Check WireGuard status
sudo wg show

# Test connectivity
ping 10.42.0.1  # WireGuard peer IP
```

### Service Discovery
```bash
# Check if service is reachable
harness daemon status
harness status
```