//! Ratatui extensions for async streaming and non-blocking rendering.
//!
//! This crate provides reusable components for building responsive TUI applications
//! that need to handle streaming data without blocking the UI event loop.
//!
//! # Features
//!
//! - **Async Stream Reader**: Non-blocking stream processing with incremental parsing
//! - **JSON Stream Parser**: Incremental JSON parsing for streaming APIs
//! - **Event Loop Integration**: Seamless integration with ratatui event loops
//!
//! # Architecture
//!
//! This crate is completely decoupled from any specific business logic. It provides
//! generic building blocks that can be composed to build responsive streaming UIs.

pub mod event_loop;
pub mod json_parser;
pub mod stream_reader;

pub use event_loop::{EventLoopConfig, NonBlockingEventLoop, StreamState};
pub use json_parser::{IncrementalJsonParser, JsonParseResult};
pub use stream_reader::{AsyncStreamReader, StreamChunk};
