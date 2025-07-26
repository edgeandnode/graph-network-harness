//! Typed action system for strongly-typed service actions
//!
//! This module provides a typed action system where actions have concrete
//! Input and Event types, returning streams of events during execution.

use async_channel::Receiver;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::Result;

/// Trait for strongly-typed actions
#[async_trait]
pub trait TypedAction: Send + Sync + 'static {
    /// Input type for this action
    type Input: DeserializeOwned + Send;

    /// Event type emitted during execution
    type Event: Serialize + Send;

    /// Get the action name
    fn name(&self) -> &'static str;

    /// Get the action description
    fn description(&self) -> &'static str;

    /// Execute the action, returning a receiver for events
    async fn execute(&self, input: Self::Input) -> Result<Receiver<Self::Event>>;
}
