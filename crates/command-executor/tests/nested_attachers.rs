//! Tests for nested attacher functionality

#[cfg(feature = "ssh")]
mod tests {
    use command_executor::attacher::{AttachConfig, AttachedHandle, Attacher};
    use command_executor::backends::local::LocalAttacher;
    use command_executor::backends::ssh::{SshAttacher, SshConfig};
    use command_executor::command::Command;
    use command_executor::target::ManagedService;

    #[test]
    fn test_ssh_attacher_type_composition() {
        // Test that we can compose SSH<Local> for attachers
        let local = LocalAttacher;
        let ssh_config = SshConfig::new("example.com").with_user("test");
        let ssh_attacher = SshAttacher::new(local, ssh_config);

        // Create a managed service target
        let mut status_cmd = Command::new("systemctl");
        status_cmd.arg("status").arg("test-service");

        let mut start_cmd = Command::new("systemctl");
        start_cmd.arg("start").arg("test-service");

        let mut stop_cmd = Command::new("systemctl");
        stop_cmd.arg("stop").arg("test-service");

        let mut log_cmd = Command::new("journalctl");
        log_cmd.arg("-u").arg("test-service").arg("-f");

        let service = ManagedService::builder("test-service")
            .status_command(status_cmd)
            .start_command(start_cmd)
            .stop_command(stop_cmd)
            .log_command(log_cmd)
            .build()
            .unwrap();

        // This should compile, proving SSH attacher can handle ManagedService targets
        let _ = (ssh_attacher, service);
    }

    #[test]
    fn test_nested_ssh_attacher_type_composition() {
        // Test that we can compose SSH<SSH<Local>> for attachers
        let local = LocalAttacher;

        let bastion_config = SshConfig::new("bastion.example.com").with_user("jumpuser");
        let ssh_to_bastion = SshAttacher::new(local, bastion_config);

        let target_config = SshConfig::new("internal.example.com").with_user("appuser");
        let ssh_to_target = SshAttacher::new(ssh_to_bastion, target_config);

        // Create a managed service
        let mut status_cmd = Command::new("systemctl");
        status_cmd.arg("status").arg("remote-service");

        let mut start_cmd = Command::new("systemctl");
        start_cmd.arg("start").arg("remote-service");

        let mut stop_cmd = Command::new("systemctl");
        stop_cmd.arg("stop").arg("remote-service");

        let mut log_cmd = Command::new("journalctl");
        log_cmd.arg("-u").arg("remote-service").arg("-f");

        let service = ManagedService::builder("remote-service")
            .status_command(status_cmd)
            .start_command(start_cmd)
            .stop_command(stop_cmd)
            .log_command(log_cmd)
            .build()
            .unwrap();

        // This should compile, proving nested attachers work
        let _ = (ssh_to_target, service);
    }

    #[test]
    fn test_attach_config_usage() {
        let config = AttachConfig {
            follow_from_start: true,
            history_lines: Some(50),
            timeout_seconds: Some(60),
        };

        // Just verify the config can be created
        assert!(config.follow_from_start);
        assert_eq!(config.history_lines, Some(50));
        assert_eq!(config.timeout_seconds, Some(60));
    }

    #[test]
    #[cfg(feature = "integration-tests")]
    fn test_ssh_attacher_execution() {
        futures::executor::block_on(async {
            let local = LocalAttacher;
            let ssh_config = SshConfig::new("localhost");
            let ssh_attacher = SshAttacher::new(local, ssh_config);

            // Create a simple service that uses commands available on most systems
            let status_cmd = Command::new("true"); // Always returns success
            let start_cmd = Command::new("true");
            let stop_cmd = Command::new("true");

            let mut log_cmd = Command::new("tail");
            log_cmd.arg("-f").arg("/dev/null");

            let service = ManagedService::builder("test-logs")
                .status_command(status_cmd)
                .start_command(start_cmd)
                .stop_command(stop_cmd)
                .log_command(log_cmd)
                .build()
                .unwrap();

            let config = AttachConfig::default();

            // Try to attach - this may fail if SSH is not configured
            let result = ssh_attacher.attach(&service, config).await;

            if let Ok((_events, mut handle)) = result {
                // Clean up
                let _ = handle.disconnect().await;
            }
        });
    }
}
