//! Health checking system for services.
//!
//! This module provides health checking functionality to monitor
//! service status and detect failures.

use crate::{config::HealthCheck, Result};
use async_trait::async_trait;
use command_executor::{backends::LocalLauncher, target::Target, Command, Executor, ProcessHandle};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Health status of a service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// Service is healthy
    Healthy,
    /// Service is unhealthy (failed health check)
    Unhealthy(String),
    /// Health check is unknown or not configured
    Unknown,
}

/// Health checker for monitoring service health
pub struct HealthChecker {
    executor: Executor<LocalLauncher>,
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self {
            executor: Executor::new("health-checker".to_string(), LocalLauncher),
        }
    }

    /// Run a single health check
    pub async fn check_health(&self, config: &HealthCheck) -> Result<HealthStatus> {
        let start = Instant::now();

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);

        debug!(
            "Running health check: {} {}",
            config.command,
            config.args.join(" ")
        );

        match self.executor.execute(&Target::Command, cmd).await {
            Ok(result) => {
                let duration = start.elapsed();

                if result.success() {
                    debug!("Health check passed in {:?}", duration);
                    Ok(HealthStatus::Healthy)
                } else {
                    let error = format!(
                        "Health check failed with exit code: {:?}",
                        result.status.code
                    );
                    warn!("Health check failed: {}", error);
                    Ok(HealthStatus::Unhealthy(error))
                }
            }
            Err(e) => {
                let error = format!("Health check execution failed: {}", e);
                warn!("Health check execution failed: {}", e);
                Ok(HealthStatus::Unhealthy(error))
            }
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for objects that can be health checked
#[async_trait]
pub trait HealthCheckable {
    /// Check the health of this object
    async fn health_check(&self) -> Result<HealthStatus>;
}

/// Continuous health monitoring for a service
pub struct HealthMonitor {
    checker: HealthChecker,
    pub(crate) config: HealthCheck,
    pub(crate) consecutive_failures: u32,
    pub(crate) last_status: HealthStatus,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(config: HealthCheck) -> Self {
        Self {
            checker: HealthChecker::new(),
            config,
            consecutive_failures: 0,
            last_status: HealthStatus::Unknown,
        }
    }

    /// Run a health check and update internal state
    pub async fn check(&mut self) -> Result<HealthStatus> {
        let status = self.checker.check_health(&self.config).await?;

        match &status {
            HealthStatus::Healthy => {
                self.consecutive_failures = 0;
                self.last_status = status.clone();
            }
            HealthStatus::Unhealthy(_) => {
                self.consecutive_failures += 1;
                // Only update status if we've exceeded retry threshold
                if self.consecutive_failures >= self.config.retries {
                    self.last_status = status.clone();
                }
            }
            HealthStatus::Unknown => {
                self.last_status = status.clone();
            }
        }

        Ok(self.last_status.clone())
    }

    /// Get the current health status
    pub fn current_status(&self) -> &HealthStatus {
        &self.last_status
    }

    /// Get the number of consecutive failures
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Get the health check interval
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.config.interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_health_status_serialization() {
        let healthy = HealthStatus::Healthy;
        let unhealthy = HealthStatus::Unhealthy("Connection refused".to_string());
        let unknown = HealthStatus::Unknown;

        // Test serialization/deserialization
        let healthy_yaml = serde_yaml::to_string(&healthy).unwrap();
        let unhealthy_yaml = serde_yaml::to_string(&unhealthy).unwrap();
        let unknown_yaml = serde_yaml::to_string(&unknown).unwrap();

        assert_eq!(healthy, serde_yaml::from_str(&healthy_yaml).unwrap());
        assert_eq!(unhealthy, serde_yaml::from_str(&unhealthy_yaml).unwrap());
        assert_eq!(unknown, serde_yaml::from_str(&unknown_yaml).unwrap());
    }

    #[test]
    fn test_health_monitor_consecutive_failures() {
        let config = HealthCheck {
            command: "false".to_string(), // Always fails
            args: vec![],
            interval: 10,
            retries: 3,
            timeout: 5,
        };

        let monitor = HealthMonitor::new(config);
        assert_eq!(monitor.consecutive_failures(), 0);
        assert_eq!(monitor.current_status(), &HealthStatus::Unknown);
    }

    #[test]
    fn test_health_checker_success() {
        smol::block_on(async {
            let checker = HealthChecker::new();
            let config = HealthCheck {
                command: "true".to_string(), // Always succeeds
                args: vec![],
                interval: 10,
                retries: 1,
                timeout: 5,
            };

            let status = checker.check_health(&config).await.unwrap();
            assert_eq!(status, HealthStatus::Healthy);
        });
    }

    #[test]
    fn test_health_checker_failure() {
        smol::block_on(async {
            let checker = HealthChecker::new();
            let config = HealthCheck {
                command: "false".to_string(), // Always fails
                args: vec![],
                interval: 10,
                retries: 1,
                timeout: 5,
            };

            let status = checker.check_health(&config).await.unwrap();
            match status {
                HealthStatus::Unhealthy(_) => {} // Expected
                _ => panic!("Expected unhealthy status"),
            }
        });
    }
}
