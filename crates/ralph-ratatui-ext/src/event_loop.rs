//! Non-blocking event loop for ratatui applications.
//!
//! Provides a framework for building responsive TUI applications that can
//! handle multiple async tasks (stream reading, user input, periodic updates)
//! without blocking.

use std::time::Duration;
use tokio::time::{MissedTickBehavior, interval};

/// Configuration for the event loop.
#[derive(Debug, Clone)]
pub struct EventLoopConfig {
    /// Tick interval for periodic updates (e.g., status bar refresh).
    pub tick_interval: Duration,
    /// Input poll interval (how often to check for user input).
    pub input_poll_interval: Duration,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(100),
            input_poll_interval: Duration::from_millis(50),
        }
    }
}

/// Non-blocking event loop coordinator.
///
/// This struct helps coordinate multiple async tasks in a ratatui application:
/// - Stream reading (stdout/stderr)
/// - User input handling
/// - Periodic UI updates
/// - Custom async tasks
///
/// # Example
///
/// ```ignore
/// let mut event_loop = NonBlockingEventLoop::new(config);
///
/// loop {
///     tokio::select! {
///         _ = event_loop.tick() => {
///             // Update UI
///         }
///         chunk = stream_reader.try_read_line() => {
///             // Process stream data
///         }
///         input = input_handler.next_event() => {
///             // Handle user input
///         }
///     }
/// }
/// ```
pub struct NonBlockingEventLoop {
    tick_interval: tokio::time::Interval,
    input_poll_interval: tokio::time::Interval,
}

impl NonBlockingEventLoop {
    /// Create a new event loop with the given configuration.
    pub fn new(config: EventLoopConfig) -> Self {
        let mut tick_interval = interval(config.tick_interval);
        tick_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut input_poll_interval = interval(config.input_poll_interval);
        input_poll_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        Self {
            tick_interval,
            input_poll_interval,
        }
    }

    /// Wait for the next tick (for periodic UI updates).
    pub async fn tick(&mut self) {
        self.tick_interval.tick().await;
    }

    /// Wait for the next input poll interval.
    pub async fn input_poll(&mut self) {
        self.input_poll_interval.tick().await;
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(EventLoopConfig::default())
    }
}

/// Helper for managing stream processing state.
///
/// Tracks whether streams are still active and provides
/// a clean way to check if all streams have closed.
#[derive(Debug, Default)]
pub struct StreamState {
    pub stdout_closed: bool,
    pub stderr_closed: bool,
    pub process_exited: bool,
}

impl StreamState {
    /// Create a new stream state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if all streams are closed and process has exited.
    pub fn is_complete(&self) -> bool {
        self.stdout_closed && self.stderr_closed && self.process_exited
    }

    /// Check if we should continue processing.
    pub fn should_continue(&self) -> bool {
        !self.process_exited || !self.stdout_closed || !self.stderr_closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_state() {
        let mut state = StreamState::new();
        assert!(!state.is_complete());
        assert!(state.should_continue());

        state.stdout_closed = true;
        assert!(!state.is_complete());
        assert!(state.should_continue());

        state.stderr_closed = true;
        assert!(!state.is_complete());
        assert!(state.should_continue());

        state.process_exited = true;
        assert!(state.is_complete());
        assert!(!state.should_continue());
    }

    #[tokio::test]
    async fn test_event_loop_tick() {
        let mut event_loop = NonBlockingEventLoop::with_defaults();

        // Should not block indefinitely
        tokio::time::timeout(Duration::from_secs(1), event_loop.tick())
            .await
            .expect("Tick should not timeout");
    }

    #[tokio::test]
    async fn test_event_loop_input_poll() {
        let mut event_loop = NonBlockingEventLoop::with_defaults();

        // Should not block indefinitely
        tokio::time::timeout(Duration::from_secs(1), event_loop.input_poll())
            .await
            .expect("Input poll should not timeout");
    }
}
