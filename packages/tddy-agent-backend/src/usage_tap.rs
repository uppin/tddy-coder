//! The usage tap: a protocol driver's raw events, the ones its spec declares as usage, appended
//! verbatim to the conversation's usage file — so an agent whose only usage source is its live
//! stream is accounted by an ordinary `jsonl` binding, and no driver names a usage field.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (`#usage-binding 3/5`)

use std::path::PathBuf;

use tddy_usage::FieldEq;

/// Where a conversation's tapped events go and which events count: the file, and the predicates
/// from the spec's `usage_events` an event must all satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageTapSpec {
    pub path: PathBuf,
    pub events: Vec<FieldEq>,
}

/// An open tap. A write that fails is logged once and disables the tap for the rest of the turn; it
/// never fails the turn.
#[derive(Debug)]
pub struct UsageTap {
    spec: UsageTapSpec,
}

impl UsageTap {
    /// A tap writing to `spec.path`, created (with its directory) on the first matching event.
    pub fn open(spec: UsageTapSpec) -> Self {
        UsageTap { spec }
    }

    /// Append `line` — one raw protocol event, as the agent wrote it — when it is JSON and matches
    /// every declared predicate.
    pub fn offer_line(&mut self, line: &str) {
        // TODO(live-tap): implement
        let _ = (&self.spec, line);
    }

    /// Append `event` — one protocol event the driver holds as a value — when it matches every
    /// declared predicate.
    pub fn offer_value(&mut self, event: &serde_json::Value) {
        // TODO(live-tap): implement
        let _ = (&self.spec, event);
    }
}
