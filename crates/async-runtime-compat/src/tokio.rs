//! Tokio runtime spawner implementation

use crate::{SpawnHandle, Spawner, SpawnerWithHandle};
use std::future::Future;
use std::pin::Pin;

/// Spawner for the Tokio runtime
#[derive(Debug, Clone, Copy)]
pub struct TokioSpawner;

impl Spawner for TokioSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        tokio::spawn(future);
    }
}

impl SpawnerWithHandle for TokioSpawner {
    type Handle = TokioHandle;

    fn spawn_with_handle(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    ) -> Self::Handle {
        TokioHandle {
            inner: tokio::spawn(future),
        }
    }
}

/// Handle to a Tokio task
pub struct TokioHandle {
    inner: tokio::task::JoinHandle<()>,
}

impl SpawnHandle for TokioHandle {
    fn detach(self) {
        // JoinHandle automatically detaches when dropped
    }

    fn abort(&self) -> Result<(), crate::UnsupportedError> {
        self.inner.abort();
        Ok(())
    }

    fn is_finished(&self) -> Result<bool, crate::UnsupportedError> {
        Ok(self.inner.is_finished())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tokio_spawner() {
        let spawner = TokioSpawner;
        let (tx, rx) = tokio::sync::oneshot::channel();

        spawner.spawn(Box::pin(async move {
            tx.send(42).unwrap();
        }));

        assert_eq!(rx.await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_tokio_spawner_with_handle() {
        let spawner = TokioSpawner;
        let (tx, rx) = tokio::sync::oneshot::channel();

        let handle = spawner.spawn_with_handle(Box::pin(async move {
            tx.send(123).unwrap();
        }));

        assert_eq!(rx.await.unwrap(), 123);
        assert!(handle.is_finished().unwrap());
    }
}
