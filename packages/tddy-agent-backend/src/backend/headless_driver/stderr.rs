//! A headless CLI's stderr: read to its end keeping the tail, and collected after the CLI exits.

use std::collections::VecDeque;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStderr;
use tokio::task::JoinHandle;

use crate::backend::headless_driver::STDERR_TAIL_LINES;
use crate::backend::turn_process::STDERR_GRACE;

/// Reads a CLI's stderr to its end, keeping the last few lines.
pub(super) async fn drain_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    let mut lines = BufReader::new(stderr).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
            Ok(None) => break,
            Err(e) => {
                log::debug!("[headless] reading stderr failed: {e}");
                break;
            }
        }
    }
    tail.into()
}

/// The stderr tail of a CLI that has exited. Waits a bounded time for the reader to see the close,
/// since a grandchild that inherited the pipe can hold it open.
pub(super) async fn stderr_tail(reader: Option<JoinHandle<Vec<String>>>) -> String {
    let Some(reader) = reader else {
        return String::new();
    };
    match tokio::time::timeout(STDERR_GRACE, reader).await {
        Ok(Ok(lines)) => lines.join("\n"),
        Ok(Err(e)) => {
            log::debug!("[headless] stderr reader failed: {e}");
            String::new()
        }
        Err(_) => {
            log::debug!("[headless] stderr still open after the CLI exited");
            String::new()
        }
    }
}
