//! Common test utilities

pub mod shared_container;
pub mod test_harness;

#[cfg(feature = "ssh")]
pub mod systemd {
    use command_executor::backends::ssh::SshConfig;
    use std::path::PathBuf;

    /// Get the path to the SSH key for testing
    fn test_ssh_key_path() -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/systemd-container/ssh-keys/test_ed25519");
        path
    }

    /// Standard SSH configuration for systemd test container
    pub fn ssh_config() -> SshConfig {
        let key_path = test_ssh_key_path();

        let mut config = SshConfig::new("localhost")
            .with_user("testuser")
            .with_port(2223)
            .with_extra_arg("-o")
            .with_extra_arg("StrictHostKeyChecking=no")
            .with_extra_arg("-o")
            .with_extra_arg("UserKnownHostsFile=/dev/null")
            .with_extra_arg("-o")
            .with_extra_arg("ConnectTimeout=5");

        // Use SSH key if it exists
        if key_path.exists() {
            config = config.with_identity_file(key_path);
        }

        config
    }

    /// Check if SSH test container is running
    pub fn is_container_running() -> bool {
        // Check for either systemd-ssh-test or simple ssh-test container
        let systemd_check = std::process::Command::new("docker")
            .args(["ps", "-q", "-f", "name=command-executor-systemd-ssh-test"])
            .output()
            .map(|output| !output.stdout.is_empty())
            .unwrap_or(false);

        let simple_check = std::process::Command::new("docker")
            .args(["ps", "-q", "-f", "name=command-executor-ssh-test"])
            .output()
            .map(|output| !output.stdout.is_empty())
            .unwrap_or(false);

        systemd_check || simple_check
    }

    /// Helper message when container is not running
    pub fn container_not_running_message() {
        eprintln!("Systemd SSH test container not running.");
        eprintln!("To start it:");
        eprintln!("  cd tests/systemd-container");
        eprintln!("  ./run-ssh-tests.sh start");
        eprintln!();
        eprintln!("To run these tests:");
        eprintln!("  ./run-ssh-tests.sh test");
    }
}
