//! Git's clone progress, as `git clone --progress` writes it to stderr: one line per redraw,
//! separated by `\r` while a stage runs and `\n` when it ends.
//!
//! ```text
//! remote: Counting objects: 100% (1200/1200), done.
//! Receiving objects:  45% (36555/81234), 38.50 MiB | 4.25 MiB/s
//! Resolving deltas: 100% (51234/51234), done.
//! ```
//!
//! [`parse`] turns one line into a [`GitProgress`]; [`ProgressThrottle`] keeps the redraws that are
//! worth sending across a pipe and a LiveKit relay.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// One parsed progress line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitProgress {
    /// Git's words for the stage, without any `remote: ` prefix.
    pub stage: String,
    pub percent: u32,
    pub objects_done: u64,
    pub objects_total: u64,
    /// Bytes received so far; 0 when the line reports none.
    pub bytes_received: u64,
    /// Throughput; 0 when the line reports none.
    pub bytes_per_second: u64,
    /// The line as git wrote it, trimmed.
    pub line: String,
}

/// Parse one progress line; `None` for a line that carries no `NN% (done/total)` progress, such as
/// `Cloning into '…'...`.
pub fn parse(line: &str) -> Option<GitProgress> {
    let line = line.trim();
    let text = line
        .strip_prefix("remote:")
        .map_or(line, |rest| rest.trim_start());
    let (stage, rest) = text.split_once(':')?;
    let stage = stage.trim();
    if stage.is_empty() {
        return None;
    }
    let (percent, rest) = rest.split_once('%')?;
    let percent = percent.trim().parse::<u32>().ok()?;
    let rest = rest.trim_start().strip_prefix('(')?;
    let (counts, tail) = rest.split_once(')')?;
    let (done, total) = counts.split_once('/')?;
    let (bytes_received, bytes_per_second) = transfer_figures(tail);
    Some(GitProgress {
        stage: stage.to_string(),
        percent,
        objects_done: done.trim().parse().ok()?,
        objects_total: total.trim().parse().ok()?,
        bytes_received,
        bytes_per_second,
        line: line.to_string(),
    })
}

/// The `, 38.50 MiB | 4.25 MiB/s` part of a line: bytes so far and bytes per second, each 0 when
/// the line does not carry it. A `, done.` segment is neither.
fn transfer_figures(tail: &str) -> (u64, u64) {
    let mut bytes = 0;
    let mut rate = 0;
    for segment in tail.split(',').map(str::trim) {
        match segment.split_once('|') {
            Some((size, speed)) => {
                bytes = byte_count(size).unwrap_or(0);
                rate = byte_count(speed.trim().trim_end_matches("/s")).unwrap_or(0);
            }
            None => {
                if let Some(size) = byte_count(segment) {
                    bytes = size;
                }
            }
        }
    }
    (bytes, rate)
}

/// `38.50 MiB` as bytes, with git's binary units.
fn byte_count(text: &str) -> Option<u64> {
    let mut words = text.split_whitespace();
    let figure: f64 = words.next()?.parse().ok()?;
    let unit: f64 = match words.next()? {
        "bytes" | "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    if words.next().is_some() {
        return None;
    }
    Some((figure * unit).round() as u64)
}

/// The shortest gap between two forwarded updates of the same stage.
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Decides which progress lines are forwarded: the first, every stage change, every 100%, and
/// otherwise at most one per [`PROGRESS_INTERVAL`].
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last: Option<(String, Instant)>,
}

impl ProgressThrottle {
    pub fn new() -> ProgressThrottle {
        ProgressThrottle::default()
    }

    /// Whether `progress`, seen at `now`, should be forwarded. Records it when it is.
    pub fn admit(&mut self, progress: &GitProgress, now: Instant) -> bool {
        let worth_sending = match &self.last {
            None => true,
            Some((stage, at)) => {
                *stage != progress.stage
                    || progress.percent >= 100
                    || now.saturating_duration_since(*at) >= PROGRESS_INTERVAL
            }
        };
        if worth_sending {
            self.last = Some((progress.stage.clone(), now));
        }
        worth_sending
    }
}
