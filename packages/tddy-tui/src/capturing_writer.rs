//! CapturingWriter: wraps a Write target and invokes a callback on each write.
//! Used to capture ratatui/crossterm terminal output for gRPC streaming.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// Callback invoked with bytes written. Must be Send for use across threads.
pub type ByteCallback = Box<dyn Fn(&[u8]) + Send>;

struct CapturingWriterInner {
    on_write: ByteCallback,
}

/// Writer that forwards to stdout and invokes a callback with each written chunk.
/// Implements Clone so it can be used with CrosstermBackend (takes ownership)
/// and separately for execute! calls (EnterAlternateScreen, etc.).
#[derive(Clone)]
pub struct CapturingWriter {
    inner: Arc<Mutex<CapturingWriterInner>>,
}

impl CapturingWriter {
    pub fn new(on_write: ByteCallback) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CapturingWriterInner { on_write })),
        }
    }
}

impl Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = std::io::stdout().write(buf)?;
        let inner = self.inner.lock().unwrap();
        (inner.on_write)(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        std::io::stdout().flush()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn write_captures_bytes() {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = captured.clone();
        let writer = CapturingWriter::new(Box::new(move |buf| {
            captured_clone.lock().unwrap().extend_from_slice(buf);
        }));

        let mut w = writer;
        let _ = w.write_all(b"hello");
        let _ = w.flush();

        let data = captured.lock().unwrap();
        assert_eq!(&data[..], b"hello");
    }

    #[test]
    fn clone_shares_callback() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_clone = count.clone();
        let writer = CapturingWriter::new(Box::new(move |_| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        let mut w1 = writer.clone();
        let mut w2 = writer;

        let _ = w1.write_all(b"a");
        let _ = w2.write_all(b"b");

        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn flush_delegates_to_stdout() {
        let writer = CapturingWriter::new(Box::new(|_| {}));
        let mut w = writer;
        let result = w.flush();
        assert!(result.is_ok());
    }
}
