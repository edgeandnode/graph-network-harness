//! Tests for the nested launcher architecture

use command_executor::backends::local::LocalLauncher;
use command_executor::command::Command;

#[cfg(feature = "ssh")]
mod ssh_tests {
    use super::*;
    use command_executor::backends::ssh::{SshConfig, SshLauncher};
    

    #[test]
    fn test_ssh_launcher_type_composition() {
        // Test that we can compose SSH<Local>
        let local = LocalLauncher;
        let ssh_config = SshConfig::new("example.com")
            .with_user("test")
            .with_port(2222);
        let ssh_launcher = SshLauncher::new(local, ssh_config);

        // The target type should be Target
        let _cmd = Command::new("echo").arg("test");

        // This should compile, proving our type system works
        let _ = ssh_launcher;
    }

    #[test]
    fn test_nested_ssh_launcher_type_composition() {
        // Test that we can compose SSH<SSH<Local>>
        let local = LocalLauncher;

        let bastion_config = SshConfig::new("bastion.example.com").with_user("jumpuser");
        let ssh_to_bastion = SshLauncher::new(local, bastion_config);

        let target_config = SshConfig::new("internal.example.com").with_user("appuser");
        let ssh_to_target = SshLauncher::new(ssh_to_bastion, target_config);

        // This should compile, proving our nested type system works
        let _ = ssh_to_target;
    }

    #[test]
    fn test_ssh_config_builder() {
        let config = SshConfig::new("test.example.com")
            .with_user("alice")
            .with_port(2222)
            .with_identity_file("/home/alice/.ssh/id_rsa")
            .with_extra_arg("-o")
            .with_extra_arg("StrictHostKeyChecking=no");

        // This test verifies the builder pattern works correctly
        let _ = config;
    }
}

#[test]
fn test_command_preparation() {
    // Test that our Command type can be used and prepared
    let mut cmd = Command::new("echo");
    cmd.arg("hello").arg("world");

    assert_eq!(cmd.get_program(), "echo");
    assert_eq!(cmd.get_args().len(), 2);

    // Test that we can prepare it for execution
    let _async_cmd = cmd.prepare();
}

#[test]
fn test_local_launcher_types() {
    // Test that LocalLauncher works with our type system
    let launcher = LocalLauncher;
    let _cmd = Command::new("echo");

    // This should compile correctly
    let _ = (launcher, _cmd);
}
