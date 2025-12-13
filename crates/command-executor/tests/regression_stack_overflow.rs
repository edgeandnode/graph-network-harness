//! Regression tests for error handling

use command_executor::error::Error;

#[test]
fn test_error_debug_formatting() {
    // Test that errors format correctly
    let error = Error::spawn_failed("base error");

    // This should not cause any issues
    let formatted = format!("{error:?}");
    println!("Formatted error: {formatted}");

    // Also test Display formatting
    let display = format!("{error}");
    println!("Display error: {display}");

    assert!(display.contains("base error"));
}
