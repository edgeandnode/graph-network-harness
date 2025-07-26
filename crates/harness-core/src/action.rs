//! Action system for extending daemon functionality
//!
//! Actions provide a way to expose custom functionality through the daemon API.
//! They are discoverable by clients and can be invoked with JSON parameters.

use async_trait::async_trait;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::{Error, Result};

/// Action function signature
pub type ActionFn = Box<
    dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<Value>> + Send + 'static>>
        + Send
        + Sync
        + 'static,
>;

/// Metadata about an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfo {
    /// Action name
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// JSON schema for parameters (optional)
    pub params_schema: Option<Value>,

    /// JSON schema for return value (optional)
    pub returns_schema: Option<Value>,

    /// Action category for organization
    pub category: Option<String>,

    /// Whether this action is deprecated
    pub deprecated: bool,
}

impl ActionInfo {
    /// Create a new action info
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            params_schema: None,
            returns_schema: None,
            category: None,
            deprecated: false,
        }
    }

    /// Set the parameter schema
    pub fn with_params_schema(mut self, schema: Value) -> Self {
        self.params_schema = Some(schema);
        self
    }

    /// Set the return value schema
    pub fn with_returns_schema(mut self, schema: Value) -> Self {
        self.returns_schema = Some(schema);
        self
    }

    /// Set the category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Mark as deprecated
    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }
}

/// Registry for managing actions
#[derive(Default)]
pub struct ActionRegistry {
    actions: IndexMap<String, ActionFn>,
    metadata: HashMap<String, ActionInfo>,
}

impl ActionRegistry {
    /// Create a new action registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an action with metadata
    pub fn register<F, Fut>(&mut self, info: ActionInfo, action: F) -> Result<()>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let name = info.name.clone();

        if self.actions.contains_key(&name) {
            return Err(Error::action(format!(
                "Action '{}' already registered",
                name
            )));
        }

        // Wrap the function to match our signature
        let action_fn = Box::new(move |params: Value| {
            Box::pin(action(params))
                as Pin<Box<dyn Future<Output = Result<Value>> + Send + 'static>>
        });

        self.actions.insert(name.clone(), action_fn);
        self.metadata.insert(name, info);

        Ok(())
    }

    /// Register a simple action without schemas
    pub fn register_simple<F, Fut>(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        action: F,
    ) -> Result<()>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value>> + Send + 'static,
    {
        let info = ActionInfo::new(name, description);
        self.register(info, action)
    }

    /// Invoke an action by name
    pub async fn invoke(&self, name: &str, params: Value) -> Result<Value> {
        let action = self
            .actions
            .get(name)
            .ok_or_else(|| Error::action(format!("Action '{}' not found", name)))?;

        action(params).await
    }

    /// Get action metadata
    pub fn get_info(&self, name: &str) -> Option<&ActionInfo> {
        self.metadata.get(name)
    }

    /// List all registered actions
    pub fn list_actions(&self) -> Vec<&ActionInfo> {
        // Return in registration order
        self.actions
            .keys()
            .filter_map(|name| self.metadata.get(name))
            .collect()
    }

    /// Get actions by category
    pub fn actions_by_category(&self, category: &str) -> Vec<&ActionInfo> {
        self.metadata
            .values()
            .filter(|info| info.category.as_deref() == Some(category))
            .collect()
    }

    /// Check if an action exists
    pub fn has_action(&self, name: &str) -> bool {
        self.actions.contains_key(name)
    }
}

/// Trait for types that can provide actions
#[async_trait]
pub trait Action {
    /// Get the action registry for this type
    fn actions(&self) -> &ActionRegistry;

    /// Get mutable action registry for registration
    fn actions_mut(&mut self) -> &mut ActionRegistry;

    /// Invoke an action
    async fn invoke_action(&self, name: &str, params: Value) -> Result<Value> {
        self.actions().invoke(name, params).await
    }

    /// List available actions
    fn list_actions(&self) -> Vec<&ActionInfo> {
        self.actions().list_actions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[smol_potat::test]
    async fn test_action_registry() {
        let mut registry = ActionRegistry::new();

        // Register a simple action
        registry
            .register_simple("test", "A test action", |params| async move {
                Ok(json!({ "received": params }))
            })
            .unwrap();

        // Invoke the action
        let result = registry
            .invoke("test", json!({"key": "value"}))
            .await
            .unwrap();
        assert_eq!(result, json!({ "received": {"key": "value"} }));

        // Check metadata
        let info = registry.get_info("test").unwrap();
        assert_eq!(info.name, "test");
        assert_eq!(info.description, "A test action");
    }

    #[smol_potat::test]
    async fn test_action_not_found() {
        let registry = ActionRegistry::new();

        let result = registry.invoke("nonexistent", json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_duplicate_registration() {
        let mut registry = ActionRegistry::new();

        let info = ActionInfo::new("test", "First");
        registry
            .register(info, |_| async { Ok(json!({})) })
            .unwrap();

        let info2 = ActionInfo::new("test", "Second");
        let result = registry.register(info2, |_| async { Ok(json!({})) });

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already registered"));
    }
}
