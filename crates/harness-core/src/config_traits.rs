//! Traits for creating services and tasks from configuration
//!
//! These traits encapsulate the logic for building services and tasks
//! from configuration, including extracting parameters and applying defaults.

use crate::{Error, Service};
use crate::task::DeploymentTask;
use service_orchestration::{ServiceConfig, TaskConfig};
use std::result::Result;

/// Trait for services that can be created from configuration
pub trait ServiceFromConfig: Service + Sized {
    /// Create a service instance from configuration
    ///
    /// This method encapsulates all logic for:
    /// - Extracting parameters from the config
    /// - Applying default values
    /// - Validating configuration
    fn from_config(config: &ServiceConfig) -> Result<Self, Error>;
}

/// Trait for tasks that can be created from configuration  
pub trait TaskFromConfig: DeploymentTask + Sized {
    /// Create a task instance from configuration
    ///
    /// This method encapsulates all logic for:
    /// - Extracting parameters from the config
    /// - Applying default values
    /// - Validating configuration
    fn from_config(config: &TaskConfig) -> Result<Self, Error>;
}