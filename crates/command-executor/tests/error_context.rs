//! Tests for error context

use command_executor::Executor;
use command_executor::Target;
use command_executor::command::Command;

#[smol_potat::test]
async fn test_local_error_context() {
    let executor = Executor::local("test-error");
    let target = Target::Command;

    let cmd = Command::new("this_command_does_not_exist_12345");

    let result = executor.execute(&target, cmd).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let err_str = err.to_string();

    // Should contain spawn failure message
    assert!(err_str.contains("spawn") || err_str.contains("Failed"));
}
