//! The context lines a `GREP` can return around each of its matches — the `before`/`after`
//! arguments, and the shape they add to every match entry.
//!
//! Split out of `subagent.rs` on the oversized-file record
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`): the argument reading,
//! the context-entry shape and the Local path's window computation are a self-contained concern
//! the turn loop and the tool engine both need spelled identically.
//!
//! The window keeps counting **matches**, not context lines — `truncated`/`total_matches` answer
//! how many matches came back and how many were left out, exactly as they do without context, so
//! a caller's windowing logic does not change shape when it asks for context
//! (`docs/dev/todo/2026-09-27-glob-and-grep-cannot-be-paged.md`: the truncation asymmetry is
//! already standing; context lines must not deepen it).

/// The greatest number of context lines one side of a match may ask for. A bound rather than
/// the model's judgement: an unbounded pair would let one call spend the window on context, the
/// very spend the match cap exists to prevent.
pub const GREP_CONTEXT_LINE_CEILING: u64 = 50;

/// One context line of a match — its line number, its text, and which side of the match it sits
/// on. `camelCase` on the wire, matching the match entries it rides.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLine {
    pub line_number: u64,
    pub text: String,
    /// `before` or `after` — spelled, not ordinal, so a reader that does not know a spelling
    /// can refuse it rather than guess a side.
    pub relation: String,
}

/// The `before`/`after` pair a `GREP` call asked for, as the tool engine's flags and the Local
/// path's window computation both read it.
///
/// `None` when the call asked for none — the result then keeps today's shape byte for byte, so a
/// caller that never asks for context never sees a key it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrepContext {
    pub before: u64,
    pub after: u64,
}

impl GrepContext {
    /// Read the `before`/`after` arguments a model sent, or `None` when it asked for none.
    ///
    /// Rejects rather than clamps a count past [`GREP_CONTEXT_LINE_CEILING`]: a clamp would
    /// answer a request the caller never made, and the model can re-ask within the bound once
    /// its tool result names it.
    pub fn from_args(args: &serde_json::Value) -> Option<Self> {
        // A non-integer or negative shape reads as absent: `validate_tool_arguments` faults one
        // before dispatch, so a call reaching here was well-formed, and guessing a window from a
        // malformed one would answer a request nobody made.
        let count = |key: &str| args.get(key).and_then(|value| value.as_u64());
        let before = count("before");
        let after = count("after");
        if before.is_none() && after.is_none() {
            return None;
        }
        let (before, after) = (before.unwrap_or(0), after.unwrap_or(0));
        if before > GREP_CONTEXT_LINE_CEILING || after > GREP_CONTEXT_LINE_CEILING {
            return None;
        }
        if before == 0 && after == 0 {
            return None;
        }
        Some(Self { before, after })
    }
}

/// Attach each match's `context` window, computed from the searched file's own lines — the Local
/// path's answer to what the engine folds out of ripgrep's `context` events, in the same
/// `{lineNumber, text, relation}` shape, so a caller cannot tell which path answered. A window
/// clamps at the file's edges: fewer lines than asked for, never padding and never an error. A
/// [`GrepContext`] of zero on both sides attaches nothing, so the entries keep their context-free
/// shape byte for byte.
pub(super) fn with_local_windows(
    mut matches: Vec<serde_json::Value>,
    context: GrepContext,
) -> Vec<serde_json::Value> {
    if context.before == 0 && context.after == 0 {
        return matches;
    }
    // One read per file, however many matches it gave: a scan's matches cluster on few files.
    let mut files: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for entry in matches.iter_mut() {
        let (Some(path), Some(line_number)) = (
            entry["data"]["path"]["text"].as_str().map(str::to_owned),
            entry["data"]["line_number"].as_u64(),
        ) else {
            continue;
        };
        let lines = files.entry(path).or_insert_with_key(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect()
        });
        let window = context_window(lines, line_number, context);
        if !window.is_empty() {
            // Serializing a struct with string fields cannot fail, and an empty window is
            // skipped above — the key is only ever present when context was asked for and found.
            entry["context"] = serde_json::to_value(window).expect("ContextLine serializes");
        }
    }
    matches
}

/// The context lines around `line_number` (1-based): up to `context.before` above, then up to
/// `context.after` below, each tagged with the side of the match it sits on.
fn context_window(lines: &[String], line_number: u64, context: GrepContext) -> Vec<ContextLine> {
    let above = usize::try_from(line_number.saturating_sub(1)).unwrap_or(usize::MAX);
    // The file can change between the scan and this read; a match line it no longer has has no
    // window to compute.
    if above >= lines.len() {
        return Vec::new();
    }
    let before_count = usize::try_from(context.before).unwrap_or(usize::MAX);
    let after_count = usize::try_from(context.after).unwrap_or(usize::MAX);
    let before_start = above.saturating_sub(before_count);
    let after_end = above
        .saturating_add(1)
        .saturating_add(after_count)
        .min(lines.len());
    let side = |i: usize, relation: &str| ContextLine {
        line_number: i as u64 + 1,
        text: lines[i].clone(),
        relation: relation.to_string(),
    };
    let before = (before_start..above).map(|i| side(i, "before"));
    let after = (above + 1..after_end).map(|i| side(i, "after"));
    before.chain(after).collect()
}
