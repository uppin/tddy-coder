//! The calls a conversation has already made, so it cannot keep making them.
//!
//! Two agents looped in session `01a0e285`, in opposite directions, and neither was caught.
//!
//! The **explorer** made 23 tool calls across **three** distinct argument sets —
//! `GhosttyTerminalSession.tsx` nine times and `scrollFollow.ts` eight, byte-identical each
//! time. Every one of them *succeeded*, so [`super::ToolCallTally`] — which fires only when
//! nothing ran at all — was blind to it by construction. Per-turn latency climbed 6s → 168s as
//! the history refilled with copies of what it already held.
//!
//! The **coder** repeated a *failing* `StrReplace` four times and was answered `old_string
//! matches 51 times (must be unique)` every time. That error names the problem, the count and
//! the rule; the agent repeated the call anyway. So this is not a diagnostics gap — it is an
//! agent that has stopped responding to information, and only the caller can break the cycle.
//!
//! The predicate is therefore "made no progress", not "everything failed", which generalises the
//! guard that already exists rather than adding a second one beside it.

use std::collections::HashMap;
use std::fmt;

use crate::agent_def::SubagentTool;

/// How many identical calls a conversation may make before the next one is refused.
///
/// Three, so the **third** is the one stopped. The second is a legitimate re-check — a model
/// confirming something it half-remembers, or re-reading after being told its edit landed — and
/// a guard that fired on it would break an ordinary habit to catch nothing. By the third, the
/// conversation is asking a question whose answer it is holding twice over, and no different
/// answer was ever coming.
pub const IDENTICAL_CALL_LIMIT: usize = 3;

/// A call refused because the conversation has already made it to the limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedCall {
    /// The model-facing tool name, as the call spelled it.
    pub tool: String,
    /// The arguments, verbatim — the other half of what makes this call the same one.
    pub arguments: String,
    /// How many times this exact call has already been dispatched.
    pub already_made: usize,
}

impl fmt::Display for RepeatedCall {
    /// What the model reads, and the only thing it will read about this call — so it says what
    /// is wrong and what to do instead, rather than simply failing. One more uninformative error
    /// is exactly what this conversation has demonstrated it will loop on.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} was not dispatched: this conversation has already made this exact call {} times, \
             and {IDENTICAL_CALL_LIMIT} identical calls is the limit. Nothing has changed the \
             codebase since the last one, so the answer would be the one already in this \
             conversation — use it, or ask something the conversation does not already hold.",
            self.tool, self.already_made
        )
    }
}

impl RepeatedCall {
    /// The `tool`-role message body for a refused repeat.
    ///
    /// Built with `serde_json` rather than assembled as text, for the reason every tool result
    /// is: the arguments quoted back are arbitrary — they are whatever the model sent — and a
    /// value carrying a quote or a newline would otherwise produce a payload that is not valid
    /// JSON on the one turn the model most needs to read it.
    pub(crate) fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "error": self.to_string(),
            "repeated_call": {
                "tool": self.tool,
                "arguments": self.arguments,
                "times_already_made": self.already_made,
            },
        })
    }
}

/// Whether a model-facing tool name is one that can change the worktree.
///
/// The name is resolved through the def vocabulary's own `serde` spelling, which is the mapping
/// [`super::tool_name`] writes and the one a def's YAML `tools:` list is read with — so a tool
/// added to the catalog is classified here without a second list to keep in step with it.
///
/// A name outside that vocabulary is not a mutation: dispatch refuses an unknown tool before it
/// can touch anything, so there is nothing for it to have made stale.
fn mutates_the_worktree(tool: &str) -> bool {
    serde_json::from_value::<SubagentTool>(serde_json::Value::String(tool.to_string()))
        .is_ok_and(SubagentTool::is_mutating)
}

/// Every call one conversation has made, keyed by tool **and** arguments.
///
/// Keyed on both because either one alone is the wrong question: a search that is actually
/// moving through a codebase issues the same tool over and over with different arguments and
/// must never be stopped for it, while the same arguments sent to a different tool ask a
/// different question and deserve their own answer.
#[derive(Debug, Default)]
pub struct RepeatedCalls {
    made: HashMap<(String, String), usize>,
}

impl RepeatedCalls {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this call may be dispatched, counting it when it may.
    ///
    /// Called **before** dispatch: a refused call must never reach the codebase, because the
    /// bytes it would fetch are bytes the conversation already holds and it would pay a turn of
    /// context for them.
    pub fn admit(&mut self, tool: &str, arguments: &str) -> Result<(), RepeatedCall> {
        let already_made = self
            .made
            .entry((tool.to_string(), arguments.to_string()))
            .or_default();
        if *already_made + 1 >= IDENTICAL_CALL_LIMIT {
            return Err(RepeatedCall {
                tool: tool.to_string(),
                arguments: arguments.to_string(),
                already_made: *already_made,
            });
        }
        *already_made += 1;
        Ok(())
    }

    /// Note how a dispatched call ended, and start the ledger over when it made every earlier
    /// answer stale.
    ///
    /// Only a mutation that **succeeded** does that, and the distinction is load-bearing in both
    /// directions. Re-reading a file you have just written is the correct thing to do, so a
    /// write that landed has to make the read that preceded it askable again. But the coder's
    /// four refused `StrReplace` calls changed nothing about the worktree, so a ledger cleared
    /// by the *attempt* would have left that loop running forever — a failed write is not news.
    ///
    /// A read that succeeded clears nothing either: it changed no file, so every earlier answer
    /// still stands.
    pub fn record_outcome(&mut self, tool: &str, succeeded: bool) {
        if succeeded && mutates_the_worktree(tool) {
            self.made.clear();
        }
    }

    /// Start over, because something outside the conversation may have moved.
    ///
    /// [`Self::record_outcome`] only sees the writes this conversation made itself, and they are
    /// not the only ones there are: between two prompts the caller has been working — it is a
    /// main agent that consults a subagent, edits, and consults it again — so a new instruction
    /// is the one moment a file this conversation read may have been rewritten under it. A
    /// ledger that survived that would bar a re-read of a file that really had changed, and the
    /// longer the session the more certainly it would be wrong.
    ///
    /// A turn that brings nothing new — a bare resume, carrying on the same question against the
    /// same history — keeps the ledger, and that is the case a loop would otherwise slip through.
    pub(crate) fn forget_earlier_calls(&mut self) {
        self.made.clear();
    }
}
