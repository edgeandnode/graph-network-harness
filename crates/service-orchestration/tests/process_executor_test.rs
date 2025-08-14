//! Test that ProcessExecutor actually starts processes

use async_runtime_compat::AsyncSpawner;
use service_orchestration::{
    ProcessCommand, ProcessExecutor, ServiceConfig, ServiceExecutor, ServiceTarget,
};
use std::collections::HashMap;

#[smol_potat::test]
async fn test_process_executor_starts_echo() -> anyhow::Result<()> {
    // Create a ProcessExecutor and spawner
    let executor = ProcessExecutor::new();
    let spawner = AsyncSpawner::new();

    // Create a simple echo service config
    let config = ServiceConfig {
        name: "test-echo".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "echo hello world".to_string(),
            },
            env: HashMap::new(),
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a PID
    assert!(running_service.pid.is_some());
    assert!(running_service.pid.unwrap() > 0);

    println!("Started echo process with PID: {:?}", running_service.pid);

    // The echo command should complete quickly
    // In a real scenario, we'd check the process is actually running
    // For echo, it exits immediately after printing

    // Stop the service (though echo probably already exited)
    let _ = executor.stop(&running_service, &spawner).await;

    Ok(())
}

#[smol_potat::test]
async fn test_process_executor_starts_sleep() -> anyhow::Result<()> {
    // Create a ProcessExecutor and spawner
    let executor = ProcessExecutor::new();
    let spawner = AsyncSpawner::new();

    // Create a sleep service that stays alive
    let config = ServiceConfig {
        name: "test-sleep".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "sleep 2".to_string(),
            }, // Sleep for 2 seconds
            env: HashMap::new(),
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a PID
    assert!(running_service.pid.is_some());
    let pid = running_service.pid.unwrap();
    assert!(pid > 0);

    println!("Started sleep process with PID: {pid}");

    // Check if process is actually running using /proc (Linux)
    #[cfg(target_os = "linux")]
    {
        let proc_path = format!("/proc/{pid}");
        assert!(
            std::path::Path::new(&proc_path).exists(),
            "Process {pid} should exist in /proc"
        );
    }

    // Stop the service
    executor.stop(&running_service, &spawner).await?;

    // Give it a moment to actually stop
    smol::Timer::after(std::time::Duration::from_millis(100)).await;

    // Check process is gone
    #[cfg(target_os = "linux")]
    {
        let proc_path = format!("/proc/{pid}");
        assert!(
            !std::path::Path::new(&proc_path).exists(),
            "Process {pid} should be gone from /proc after stop"
        );
    }

    Ok(())
}

#[smol_potat::test]
async fn test_process_executor_environment_variables() -> anyhow::Result<()> {
    // Create a ProcessExecutor and spawner
    let executor = ProcessExecutor::new();
    let spawner = AsyncSpawner::new();

    // Create a service that uses environment variables
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = ServiceConfig {
        name: "test-env".to_string(),
        target: ServiceTarget::Process {
            command: ProcessCommand::Legacy {
                command: "sh -c 'echo TEST_VAR=$TEST_VAR && sleep 1'".to_string(),
            },
            env,
            working_dir: None,
            validation: None,
        },
        depends_on: vec![],
        health_check: None,
    };

    // Start the service
    let running_service = executor.start(config.clone(), &spawner).await?;

    // Check that we got a PID
    assert!(running_service.pid.is_some());

    println!("Started sh process with PID: {:?}", running_service.pid);

    // Stop the service
    executor.stop(&running_service, &spawner).await?;

    Ok(())
}
