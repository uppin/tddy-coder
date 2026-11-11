//! The usage tap: a protocol driver's raw events, the ones its spec declares as usage, appended
//! verbatim to the conversation's usage file — so an agent whose only usage source is its live
//! stream is accounted by an ordinary `jsonl` binding, and no driver names a usage field.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (`#usage-binding 3/5`)

use std::io::Write;
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
    disabled: bool,
}

impl UsageTap {
    /// A tap writing to `spec.path`, created (with its directory) on the first matching event.
    pub fn open(spec: UsageTapSpec) -> Self {
        UsageTap {
            spec,
            disabled: false,
        }
    }

    /// Append `line` — one raw protocol event, as the agent wrote it — when it is JSON and matches
    /// every declared predicate.
    pub fn offer_line(&mut self, line: &str) {
        if self.disabled || self.spec.events.is_empty() {
            return;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if self.matches(&event) {
            self.append(line);
        }
    }

    /// Append `event` — one protocol event the driver holds as a value — when it matches every
    /// declared predicate.
    pub fn offer_value(&mut self, event: &serde_json::Value) {
        if self.disabled || !self.matches(event) {
            return;
        }
        self.append(&event.to_string());
    }

    /// The event satisfies every predicate. A tap declaring none taps nothing.
    fn matches(&self, event: &serde_json::Value) -> bool {
        !self.spec.events.is_empty()
            && self.spec.events.iter().all(|predicate| {
                predicate
                    .field
                    .lookup(event)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|actual| actual == predicate.equals)
            })
    }

    fn append(&mut self, line: &str) {
        if let Err(e) = self.write_line(line) {
            log::warn!(
                "usage tap: cannot write {}: {e}; the tap is off for this turn",
                self.spec.path.display()
            );
            self.disabled = true;
        }
    }

    fn write_line(&self, line: &str) -> std::io::Result<()> {
        if let Some(dir) = self.spec.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.spec.path)?;
        file.write_all(format!("{line}\n").as_bytes())
    }
}
