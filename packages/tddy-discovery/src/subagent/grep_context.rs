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
        // TODO(grep-context): read `before`/`after`, reject past the ceiling, None when absent.
        let _ = args;
        None
    }
}
