//! Utility functions for event streaming

use super::EventStream;
use command_executor::event::ProcessEvent;
use futures::{
    stream::{self, Stream, StreamExt},
    lock::Mutex,
};
use std::sync::Arc;

/// Type alias for the shared event stream used by executors
pub type SharedEventStream = Arc<Mutex<Box<dyn Stream<Item = ProcessEvent> + Send + Unpin>>>;

/// Create a forwarding stream from a shared event stream
/// 
/// This is used by executors to create new event streams that forward
/// events from the stored stream without consuming it.
pub fn create_forwarding_stream(event_stream: SharedEventStream) -> EventStream {
    let log_stream = stream::unfold(event_stream, |event_stream| async {
        let next_event = {
            let mut stream = event_stream.lock().await;
            stream.next().await
        };
        match next_event {
            Some(event) => Some((event, event_stream)),
            None => None,
        }
    });
    
    Box::pin(log_stream)
}