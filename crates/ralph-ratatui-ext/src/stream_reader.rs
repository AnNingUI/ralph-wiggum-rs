//! Async stream reader for non-blocking data consumption.
//!
//! Provides a generic way to read from async streams (stdout, stderr, network, etc.)
//! without blocking the UI event loop.

use anyhow::Result;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// A chunk of data read from a stream.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A complete line (newline-delimited).
    Line(String),
    /// Raw bytes (for non-line-based protocols).
    Bytes(Vec<u8>),
    /// End of stream.
    Eof,
}

/// Async stream reader that can be polled without blocking.
///
/// This reader wraps any `AsyncBufRead` and provides non-blocking access
/// to stream data. It's designed to be used in a `tokio::select!` loop
/// alongside UI event handling.
pub struct AsyncStreamReader<R: AsyncBufRead + Unpin> {
    reader: R,
    buffer: String,
    eof: bool,
}

impl<R: AsyncBufRead + Unpin> AsyncStreamReader<R> {
    /// Create a new async stream reader.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: String::new(),
            eof: false,
        }
    }

    /// Try to read the next line without blocking.
    ///
    /// Returns `None` if no complete line is available yet.
    /// Returns `Some(StreamChunk::Line)` if a line was read.
    /// Returns `Some(StreamChunk::Eof)` if the stream has ended.
    pub async fn try_read_line(&mut self) -> Result<Option<StreamChunk>> {
        if self.eof {
            return Ok(None);
        }

        self.buffer.clear();
        match self.reader.read_line(&mut self.buffer).await {
            Ok(0) => {
                self.eof = true;
                Ok(Some(StreamChunk::Eof))
            }
            Ok(_) => {
                let line = self.buffer.trim_end_matches('\n').to_string();
                Ok(Some(StreamChunk::Line(line)))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Check if the stream has reached EOF.
    pub fn is_eof(&self) -> bool {
        self.eof
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_read_lines() {
        let data = b"line1\nline2\nline3\n";
        let reader = BufReader::new(&data[..]);
        let mut stream = AsyncStreamReader::new(reader);

        let chunk1 = stream.try_read_line().await.unwrap();
        assert!(matches!(chunk1, Some(StreamChunk::Line(ref s)) if s == "line1"));

        let chunk2 = stream.try_read_line().await.unwrap();
        assert!(matches!(chunk2, Some(StreamChunk::Line(ref s)) if s == "line2"));

        let chunk3 = stream.try_read_line().await.unwrap();
        assert!(matches!(chunk3, Some(StreamChunk::Line(ref s)) if s == "line3"));

        let eof = stream.try_read_line().await.unwrap();
        assert!(matches!(eof, Some(StreamChunk::Eof)));
    }

    #[tokio::test]
    async fn test_eof_flag() {
        let data = b"test\n";
        let reader = BufReader::new(&data[..]);
        let mut stream = AsyncStreamReader::new(reader);

        assert!(!stream.is_eof());
        stream.try_read_line().await.unwrap();
        assert!(!stream.is_eof());
        stream.try_read_line().await.unwrap();
        assert!(stream.is_eof());
    }
}
