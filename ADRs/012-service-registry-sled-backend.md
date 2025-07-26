# ADR-012: Service Registry Sled Backend

## Status
Accepted

## Context

The current service registry uses JSON file persistence with several limitations:

1. **Full file rewrites** - Every update serializes and writes the entire registry
2. **No concurrent access** - Multiple processes cannot safely share a registry
3. **Manual crash safety** - Uses temp file + rename pattern for atomicity
4. **No partial updates** - Small changes require rewriting everything
5. **No schema evolution** - Adding fields breaks backward compatibility

As the harness evolves toward production use cases with the library-first architecture (ADR-010), we need:
- Multiple daemon processes sharing service discovery
- Reliable persistence that survives crashes
- Efficient updates for high-frequency health checks
- Schema evolution for adding new service metadata
- Zero-downtime upgrades

## Decision

We will migrate the service registry to use [sled](https://github.com/spacejam/sled) as an embedded database with [fcode](https://github.com/tijsvd/fcode) for serialization.

### Why Sled?

Sled is a modern embedded database written in pure Rust that provides:
- **ACID transactions** - Atomic updates with automatic crash recovery
- **Concurrent access** - Lock-free architecture for multiple readers/writers  
- **Efficient updates** - Only changed pages are written to disk
- **Compression** - Automatic Zstd compression for space efficiency
- **Pure Rust** - No C dependencies, easy to build and deploy

### Why Fcode?

Fcode is a binary serialization format that provides:
- **Schema evolution** - Add fields without breaking existing data
- **Forward compatibility** - Old versions can read new data (skip unknown fields)
- **Compact encoding** - Similar size to Protocol Buffers
- **Serde integration** - Works with our existing derives
- **Better than bincode** - Designed for long-lived data, not just RPC

## Implementation

### Architecture

```rust
use sled::Db;
use fcode::{Encoder, Decoder};

pub struct Registry {
    /// Sled database instance
    db: Db,
    /// Service entries tree
    services: sled::Tree,
    /// Event log tree (for recovery/audit)
    events: sled::Tree,
    /// Subscribers for WebSocket events
    subscribers: Arc<Mutex<HashMap<SocketAddr, EventSubscription>>>,
}

impl Registry {
    pub fn new() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()?;
            
        Ok(Self {
            services: db.open_tree("services")?,
            events: db.open_tree("events")?,
            db,
            subscribers: Default::default(),
        })
    }
    
    pub fn with_persistence(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::Config::new()
            .path(path)
            .mode(0o600)  // Secure permissions
            .cache_capacity(64 * 1024 * 1024)  // 64MB cache
            .flush_every_ms(Some(1000))  // Flush every second
            .open()?;
            
        Ok(Self {
            services: db.open_tree("services")?,
            events: db.open_tree("events")?,
            db,
            subscribers: Default::default(),
        })
    }
}
```

### Service Operations

```rust
impl Registry {
    pub async fn register(&self, entry: ServiceEntry) -> Result<()> {
        let key = entry.name.as_bytes();
        
        // Serialize with fcode
        let mut encoder = Encoder::new();
        entry.encode(&mut encoder)?;
        let value = encoder.finish();
        
        // Atomic insert with event logging
        self.db.transaction(|tx_db| {
            // Update service entry
            tx_db.open_tree("services")?.insert(key, value)?;
            
            // Log event for audit/recovery
            let event = RegistrationEvent {
                service: entry.name.clone(),
                timestamp: Utc::now(),
                version: entry.version.clone(),
            };
            
            let event_key = format!("{}-{}", entry.name, Utc::now().timestamp_nanos());
            let mut event_encoder = Encoder::new();
            event.encode(&mut event_encoder)?;
            
            tx_db.open_tree("events")?.insert(
                event_key.as_bytes(),
                event_encoder.finish()
            )?;
            
            Ok(())
        })?;
        
        // Emit WebSocket events (outside transaction)
        self.emit_event(EventType::ServiceRegistered, json!({
            "service": entry.name,
            "version": entry.version,
        })).await;
        
        Ok(())
    }
    
    pub async fn get(&self, name: &str) -> Result<Option<ServiceEntry>> {
        let key = name.as_bytes();
        
        match self.services.get(key)? {
            Some(value) => {
                let mut decoder = Decoder::new(&value);
                let entry = ServiceEntry::decode(&mut decoder)?;
                Ok(Some(entry))
            }
            None => Ok(None)
        }
    }
}
```

### Migration Strategy

```rust
impl Registry {
    /// Migrate from JSON file to sled
    pub async fn migrate_from_json(json_path: &Path, sled_path: &Path) -> Result<()> {
        // Load old JSON data
        let json_data = async_fs::read_to_string(json_path).await?;
        let old_services: HashMap<String, ServiceEntry> = 
            serde_json::from_str(&json_data)?;
        
        // Create new sled registry
        let registry = Self::with_persistence(sled_path)?;
        
        // Import all services
        for (_, entry) in old_services {
            registry.register(entry).await?;
        }
        
        info!("Migrated {} services to sled", registry.services.len());
        Ok(())
    }
}
```

### Concurrent Access Pattern

```rust
// Multiple daemon processes can share the same registry
let registry1 = Registry::with_persistence("/var/lib/harness/registry")?;
let registry2 = Registry::with_persistence("/var/lib/harness/registry")?;

// Both can read/write concurrently - sled handles coordination
registry1.register(service_a).await?;
registry2.register(service_b).await?;

// Changes are immediately visible to all processes
let service_a_from_2 = registry2.get("service_a").await?;
```

## Schema Evolution Example

```rust
// Version 1.0
#[derive(Encode, Decode)]
struct ServiceEntry {
    name: String,
    version: String,
    state: ServiceState,
}

// Version 1.1 - Added new field
#[derive(Encode, Decode)]
struct ServiceEntry {
    name: String,
    version: String,
    state: ServiceState,
    #[fcode(default)]  // New field with default
    health_endpoint: Option<String>,
}

// Version 1.0 can still read 1.1 data (ignores health_endpoint)
// Version 1.1 can read 1.0 data (health_endpoint = None)
```

## Consequences

### Positive

1. **Production Ready**: ACID transactions, crash recovery, concurrent access
2. **Performance**: 
   - Reads: O(log n) B-tree lookups with caching
   - Writes: Only dirty pages flushed to disk
   - No full-file rewrites
3. **Schema Evolution**: Add fields without migrations
4. **Reliability**: Automatic recovery from crashes
5. **Monitoring**: Built-in metrics for cache hits, flushes, etc.
6. **Multi-Process**: Multiple daemons can share discovery
7. **Audit Trail**: Event log tree for debugging/compliance

### Negative

1. **New Dependencies**: sled (~200KB) and fcode (~50KB)
2. **Serialization Overhead**: fcode is ~2x slower than bincode
3. **Disk Space**: B-tree structure uses more space than JSON
4. **Breaking Change**: Requires migration from existing JSON files
5. **Complexity**: More complex than simple file I/O

### Neutral

1. **Binary Format**: fcode data isn't human-readable (tools needed)
2. **Backup Format**: Need to export to JSON for portability
3. **Memory Usage**: Configurable cache size (default 64MB)

## Migration Plan

### Phase 1: Add Sled Backend (Backward Compatible)
1. Add sled and fcode dependencies
2. Implement `SledRegistry` alongside existing `Registry`
3. Add feature flag to choose backend
4. Add migration command to CLI

### Phase 2: Default to Sled (With Fallback)
1. Make sled the default backend
2. Auto-migrate from JSON if found
3. Keep JSON export for backup
4. Monitor for issues

### Phase 3: Remove JSON Backend
1. After 2-3 releases, remove JSON code
2. Sled becomes the only backend
3. Simpler codebase

## Example CLI Usage

```bash
# Current JSON registry (backward compatible)
harness daemon start --registry-backend json

# New sled registry (future default)
harness daemon start --registry-backend sled

# Migrate existing registry
harness registry migrate \
  --from /var/lib/harness/registry.json \
  --to /var/lib/harness/registry.sled

# Export for backup/debugging
harness registry export \
  --from /var/lib/harness/registry.sled \
  --to registry-backup.json
```

## Alternatives Considered

1. **RocksDB**: More mature but requires C++ bindings
2. **LMDB**: Very fast reads but complex API and less Rust-idiomatic
3. **SQLite**: Full SQL but overkill for key-value storage
4. **Redb**: Pure Rust like sled but less mature
5. **Bincode**: Faster serialization but no schema evolution

## References

- [Sled Documentation](https://docs.rs/sled)
- [Fcode Repository](https://github.com/tijsvd/fcode)
- [ADR-010: Domain-Oriented Action System](./010-domain-oriented-action-system.md) - Requires multi-process coordination
- [Why Schema Evolution Matters](https://martin.kleppmann.com/2012/12/05/schema-evolution-in-avro-protocol-buffers-thrift.html)

## Decision Outcome

Migrating to sled with fcode provides a production-ready service registry that supports concurrent access, schema evolution, and crash recovery. This aligns with the harness evolution toward a library-first architecture where multiple specialized daemons need reliable service discovery.
