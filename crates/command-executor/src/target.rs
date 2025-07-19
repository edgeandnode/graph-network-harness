//! Configuration-level execution target type

use std::path::PathBuf;

// TODO: This enum belongs in the configuration/service-registry layer
// It will be used for deserializing from YAML and converting to
// the appropriate target struct for the chosen backend

/// Configuration-level execution target specification
#[derive(Debug, Clone)]
pub enum ExecutionTarget {
    /// Execute as a one-off command
    Command,
    
    /// Execute as a managed process (we track PID and lifecycle)
    ManagedProcess {
        /// Optional process group ID
        process_group: Option<i32>,
        /// Whether to restart on failure
        restart_on_failure: bool,
    },
    
    /// Execute via systemd (systemctl commands)
    SystemdService {
        /// The systemd unit name
        unit_name: String,
    },
    
    /// Execute via systemd-portable (portablectl commands)
    SystemdPortable {
        /// The portable service image name
        image_name: String,
        /// The systemd unit name
        unit_name: String,
    },
    
    /// Execute inside a Docker container
    DockerContainer {
        /// Container ID or name
        container: String,
    },
    
    /// Execute as part of a docker-compose service
    ComposeService {
        /// Path to docker-compose.yml
        compose_file: PathBuf,
        /// Service name in the compose file
        service: String,
    },
}