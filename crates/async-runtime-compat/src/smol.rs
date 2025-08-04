//! Smol runtime spawner implementation

use crate::{Spawner, SpawnerWithHandle, SpawnHandle};
use std::future::Future;
use std::pin::Pin;

/// Spawner for the Smol runtime
#[derive(Debug, Clone, Copy)]
pub struct SmolSpawner;

impl Spawner for SmolSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        smol::spawn(future).detach();
    }
}

impl SpawnerWithHandle for SmolSpawner {
    type Handle = SmolHandle;
    
    fn spawn_with_handle(
        &self, 
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>
    ) -> Self::Handle {
        SmolHandle {
            inner: Some(smol::spawn(future)),
        }
    }
}

/// Handle to a Smol task
pub struct SmolHandle {
    inner: Option<smol::Task<()>>,
}

impl SpawnHandle for SmolHandle {
    fn detach(mut self) {
        if let Some(task) = self.inner.take() {
            task.detach();
        }
    }
}

impl Drop for SmolHandle {
    fn drop(&mut self) {
        if let Some(task) = self.inner.take() {
            task.detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[smol_potat::test]
    async fn test_smol_spawner() {
        let spawner = SmolSpawner;
        let (tx, rx) = async_channel::bounded(1);
        
        spawner.spawn(Box::pin(async move {
            tx.send(42).await.unwrap();
        }));
        
        assert_eq!(rx.recv().await.unwrap(), 42);
    }
    
    #[smol_potat::test]
    async fn test_smol_spawner_with_handle() {
        let spawner = SmolSpawner;
        let (tx, rx) = async_channel::bounded(1);
        
        let handle = spawner.spawn_with_handle(Box::pin(async move {
            tx.send(123).await.unwrap();
        }));
        
        assert_eq!(rx.recv().await.unwrap(), 123);
        handle.detach();
    }
}