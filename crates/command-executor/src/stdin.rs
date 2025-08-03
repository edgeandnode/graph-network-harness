//! Stdin handling for processes
//!
//! This module provides the `StdinHandle` type for writing to a process's stdin.
//! It supports both direct writing and channel-based forwarding of input.

use async_channel::Receiver;
use futures::io::AsyncWriteExt;
use crate::error::Result;

/// Handle for writing to a process's stdin
pub struct StdinHandle {
    /// The actual stdin writer
    stdin: Option<async_process::ChildStdin>,
    /// Optional channel to receive input from
    channel: Option<Receiver<String>>,
}

impl StdinHandle {
    /// Create a new stdin handle
    pub fn new(stdin: async_process::ChildStdin, channel: Option<Receiver<String>>) -> Self {
        Self { stdin: Some(stdin), channel }
    }
    
    /// Write a line to stdin (adds newline)
    pub async fn write_line(&mut self, line: &str) -> Result<()> {
        if let Some(stdin) = &mut self.stdin {
            stdin.write_all(line.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }
        Ok(())
    }
    
    /// Write raw bytes to stdin
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        if let Some(stdin) = &mut self.stdin {
            stdin.write_all(data).await?;
            stdin.flush().await?;
        }
        Ok(())
    }
    
    /// Start forwarding from the channel to stdin
    /// This consumes self and runs until the channel is closed
    pub async fn forward_channel(mut self) -> Result<()> {
        if let Some(channel) = self.channel.take() {
            while let Ok(line) = channel.recv().await {
                self.write_line(&line).await?;
            }
        }
        Ok(())
    }
    
    /// Take the channel, leaving None in its place
    pub fn take_channel(&mut self) -> Option<Receiver<String>> {
        self.channel.take()
    }
    
    /// Close stdin by dropping the writer
    pub fn close(&mut self) {
        self.stdin.take();
    }
}