//! Standalone VT100 screen parsing and echo assertion helpers for tests (gRPC/LiveKit streams,
//! raw ANSI buffers). Wraps [`vt100::Parser`] without requiring [`crate::TuiTestkit`].

use std::sync::{Arc, Mutex};
use std::time::Duration;

use vt100::Parser;

/// Incremental VT100 parser for ANSI terminal output (same model as Ghostty / in-process VirtualTui).
pub struct ScreenParser {
    parser: Parser,
}

impl ScreenParser {
    /// Create a parser for a terminal of `rows` × `cols` with no scrollback buffer.
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: Parser::new(rows, cols, 0),
        }
    }

    /// Feed raw ANSI bytes into the parser state machine.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Flattened visible screen text (rows joined with newlines), as returned by vt100.
    pub fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    /// Visible screen with all whitespace removed (used for large-echo substring checks).
    pub fn compact_contents(&self) -> String {
        self.contents()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }

    /// Whether the visible screen contains `text` as a substring.
    pub fn contains(&self, text: &str) -> bool {
        self.contents().contains(text)
    }
}

/// One-shot parse: VT100 model of `bytes` at `rows`×`cols`, then whitespace-stripped screen string.
pub fn compact_screen(bytes: &[u8], rows: u16, cols: u16) -> String {
    let mut p = ScreenParser::new(rows, cols);
    p.feed(bytes);
    p.compact_contents()
}

/// Normalize the flattened VT100 string before echo substring checks:
///
/// - Idle pulse glyphs (`·` / `•` / `●`) on the status line.
/// - Mouse **Enter** affordance (`paint_enter_affordance`): light box-drawing border (`┌─` … `│` …)
///   and U+23CE on the first prompt text row; legacy ASCII `+--` / `|` may still appear in old logs.
///
/// These glyphs overlay the last columns of wrapped prompt lines and break naive contiguous-prefix checks.
pub fn compact_screen_for_echo_assertions(compact: &str) -> String {
    let mut s: String = compact
        .chars()
        .filter(|&c| !matches!(c, '·' | '•' | '●'))
        .collect();
    while s.contains("+--") {
        s = s.replace("+--", "");
    }
    s.chars()
        .filter(|&c| {
            !matches!(c, '|' | '\u{23CE}')
                && !matches!(
                    c,
                    '\u{2500}' | '\u{2502}' | '\u{250C}' | '\u{2510}' | '\u{2514}' | '\u{2518}'
                )
        })
        .collect()
}

/// Longest prefix length of `expected_no_ws` found as a substring in echo-normalized compact screen.
pub fn longest_echo_prefix_len_in_compact(compact: &str, expected_no_ws: &str) -> usize {
    let normalized = compact_screen_for_echo_assertions(compact);
    let mut lo = 0usize;
    let mut hi = expected_no_ws.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if normalized.contains(&expected_no_ws[..mid]) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Longest prefix on whitespace-stripped compact screen only (no idle/affordance normalization).
/// Used for LiveKit large-echo parity with historical `lk_longest_echo_prefix`.
pub fn longest_echo_prefix_raw_compact(compact: &str, expected_no_ws: &str) -> usize {
    let mut lo = 0usize;
    let mut hi = expected_no_ws.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if compact.contains(&expected_no_ws[..mid]) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Whether the full expected echo (no-whitespace form) appears contiguously in the VT100 parse.
pub fn segmented_echo_complete(
    all_output: &[u8],
    expected_full: &str,
    rows: u16,
    cols: u16,
    style: SegmentedEchoFailureStyle,
) -> bool {
    let compact = compact_screen(all_output, rows, cols);
    let expected_no_ws: String = expected_full
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    match style {
        SegmentedEchoFailureStyle::Grpc => {
            longest_echo_prefix_len_in_compact(&compact, &expected_no_ws) == expected_no_ws.len()
        }
        SegmentedEchoFailureStyle::LiveKit => {
            longest_echo_prefix_raw_compact(&compact, &expected_no_ws) == expected_no_ws.len()
        }
    }
}

/// How [`assert_segmented_echo`] formats failure messages and whether echo-assertion normalization
/// is applied (gRPC uses full normalization; LiveKit large-echo tests used raw compact only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentedEchoFailureStyle {
    /// gRPC e2e: [`compact_screen_for_echo_assertions`] + `region_hint` in panic message.
    Grpc,
    /// LiveKit e2e: whitespace-stripped compact only; shorter panic message.
    LiveKit,
}

/// Tunable polling parameters for [`eventually_segmented_echo`].
#[derive(Clone, Copy, Debug)]
pub struct SegmentedEchoWaitParams {
    pub timeout: Duration,
    pub min_interval: Duration,
    pub min_new_bytes: usize,
    pub loop_sleep: Duration,
    pub style: SegmentedEchoFailureStyle,
}

/// Poll until `segmented_echo_complete` returns true or the wait params' timeout elapses.
/// Throttles expensive full parses (`min_interval` and/or `min_new_bytes`).
pub async fn eventually_segmented_echo(
    buf: &Arc<Mutex<Vec<u8>>>,
    expected_full: &str,
    rows: u16,
    cols: u16,
    wait: SegmentedEchoWaitParams,
) -> bool {
    let deadline = tokio::time::Instant::now() + wait.timeout;
    let mut last_check_at = tokio::time::Instant::now() - wait.min_interval;
    let mut last_check_len = 0usize;

    while tokio::time::Instant::now() < deadline {
        let ok = {
            let g = buf.lock().expect("segmented echo sync buffer");
            let len = g.len();
            let due = last_check_at.elapsed() >= wait.min_interval
                || len.saturating_sub(last_check_len) >= wait.min_new_bytes;
            if due {
                last_check_at = tokio::time::Instant::now();
                last_check_len = len;
                segmented_echo_complete(&g, expected_full, rows, cols, wait.style)
            } else {
                false
            }
        };
        if ok {
            return true;
        }
        tokio::time::sleep(wait.loop_sleep).await;
    }

    let g = buf.lock().expect("segmented echo sync buffer");
    segmented_echo_complete(&g, expected_full, rows, cols, wait.style)
}

/// Assert the full segmented payload is visible in the VT100 model with detailed diagnostics.
pub fn assert_segmented_echo(
    all_output: &[u8],
    expected_full: &str,
    segments: &[String],
    rows: u16,
    cols: u16,
    failure_style: SegmentedEchoFailureStyle,
) {
    let compact_raw = compact_screen(all_output, rows, cols);
    let compact = match failure_style {
        SegmentedEchoFailureStyle::Grpc => compact_screen_for_echo_assertions(&compact_raw),
        SegmentedEchoFailureStyle::LiveKit => compact_raw.clone(),
    };
    let expected_no_ws: String = expected_full
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let mut seg_full_in_compact: Vec<bool> = Vec::with_capacity(segments.len());
    let mut seg_marker_in_compact: Vec<bool> = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        let seg_no_ws: String = seg.chars().filter(|c| !c.is_whitespace()).collect();
        seg_full_in_compact.push(compact.contains(&seg_no_ws));
        let marker = format!("#SEG-{}:", i);
        seg_marker_in_compact.push(compact.contains(marker.as_str()));
    }

    let lo = match failure_style {
        SegmentedEchoFailureStyle::Grpc => {
            longest_echo_prefix_len_in_compact(&compact, &expected_no_ws)
        }
        SegmentedEchoFailureStyle::LiveKit => {
            longest_echo_prefix_raw_compact(&compact, &expected_no_ws)
        }
    };

    let missing_full: Vec<usize> = seg_full_in_compact
        .iter()
        .enumerate()
        .filter(|(_, ok)| !**ok)
        .map(|(i, _)| i)
        .collect();
    let missing_markers: Vec<usize> = seg_marker_in_compact
        .iter()
        .enumerate()
        .filter(|(_, ok)| !**ok)
        .map(|(i, _)| i)
        .collect();

    let last_idx = segments.len().saturating_sub(1);
    let region_hint = if missing_full.is_empty() {
        "all segment bodies visible as substrings"
    } else if missing_full.contains(&0) {
        "leading: segment 0 missing (start of echoed input not present as substring)"
    } else if missing_full.len() == 1 && missing_full[0] == last_idx {
        if seg_marker_in_compact[last_idx] && !seg_full_in_compact[last_idx] {
            "trailing: last #SEG marker is present but the tail of that segment (after the marker) is not present as one contiguous substring"
        } else {
            "trailing: only the last segment missing (marker and body)"
        }
    } else if missing_full.iter().all(|&i| i > 0) && missing_full.iter().any(|&i| i < last_idx) {
        "middle: some interior segment(s) missing (first missing index > 0)"
    } else {
        "mixed: see missing_full indices vs segment count"
    };

    let label = match failure_style {
        SegmentedEchoFailureStyle::Grpc => "vt100 contiguous echo check failed",
        SegmentedEchoFailureStyle::LiveKit => "livekit vt100 echo check failed",
    };

    match failure_style {
        SegmentedEchoFailureStyle::Grpc => {
            assert_eq!(
                lo,
                expected_no_ws.len(),
                "{}.\n\
                 longest prefix (no ws) found: {} of {}\n\
                 per-segment full body in compact: {:?} (indices 0..{})\n\
                 per-segment #SEG-n: marker in compact: {:?}\n\
                 segments missing as full substring: {:?}\n\
                 markers missing: {:?}\n\
                 region hint: {}\n",
                label,
                lo,
                expected_no_ws.len(),
                seg_full_in_compact,
                segments.len(),
                seg_marker_in_compact,
                missing_full,
                missing_markers,
                region_hint
            );
        }
        SegmentedEchoFailureStyle::LiveKit => {
            assert_eq!(
                lo,
                expected_no_ws.len(),
                "{}.\n\
                 longest prefix (no ws) found: {} of {}\n\
                 per-segment full body in compact: {:?} (indices 0..{})\n\
                 per-segment #SEG-n: marker in compact: {:?}\n\
                 segments missing as full substring: {:?}\n\
                 markers missing: {:?}\n",
                label,
                lo,
                expected_no_ws.len(),
                seg_full_in_compact,
                segments.len(),
                seg_marker_in_compact,
                missing_full,
                missing_markers,
            );
        }
    }
}
