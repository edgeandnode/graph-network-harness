//! async-std runtime spawner implementation

use crate::{SpawnHandle, Spawner, SpawnerWithHandle};
use std::future::Future;
use std::pin::Pin;

/// Spawner for the async-std runtime
#[derive(Debug, Clone, Copy)]
pub struct AsyncStdSpawner;

impl Spawner for AsyncStdSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        async_std::task::spawn(future);
    }
}

impl SpawnerWithHandle for AsyncStdSpawner {
    type Handle = AsyncStdHandle;

    fn spawn_with_handle(
        &self,
        future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    ) -> Self::Handle {
        AsyncStdHandle {
            inner: async_std::task::spawn(future),
        }
    }
}

/// Handle to an async-std task
pub struct AsyncStdHandle {
    inner: async_std::task::JoinHandle<()>,
}

impl SpawnHandle for AsyncStdHandle {
    fn detach(self) {
        // JoinHandle automatically detaches when dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn test_async_std_spawner() {
        let spawner = AsyncStdSpawner;
        let (tx, rx) = async_channel::bounded(1);

        spawner.spawn(Box::pin(async move {
            tx.send(42).await.unwrap();
        }));

        assert_eq!(rx.recv().await.unwrap(), 42);
    }
}
