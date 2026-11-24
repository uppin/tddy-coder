//! `coding_agent.model_report`: where a running agent reports the model it is on *now*, so a
//! session's record follows a model its operator picked inside the agent (Cursor's or Claude Code's
//! `/model`) and a resume relaunches on it rather than on the start's.
//!
//! ```yaml
//! model_report:
//!   - { kind: hook_field, field: model }                        # every hook event's `model`
//!   - { kind: hook_field, field: model, events: [SessionStart] }
//!   - { kind: usage, events: [Stop], ignore: ["<synthetic>"] }  # the transcript's last model
//! ```
//!
//! The sources are tried in order on every hook report; the first that applies to the report's
//! event and yields a model wins. An agent without the slot reports nothing, and its record keeps
//! the start's model. Both agents' own `/model` also rewrites a *global* default, so dropping
//! `--model` on resume is no substitute: only the session's own report says what *it* was on.

use serde::{Deserialize, Serialize};

/// One place the agent reports its current model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelReportSource {
    /// A top-level string field of the hook event's JSON (`ReportSessionStatus.hook_fields`).
    HookField {
        field: String,
        /// The hook events it is read on; empty for every event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        events: Vec<String>,
        /// Values that are not a model to relaunch on.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ignore: Vec<String>,
    },
    /// The model the session's usage binding (`coding_agent.usage`) reads last off the main
    /// conversation's transcript, evaluated by `tddy-usage`.
    Usage {
        /// The hook events it is read on; empty for every event.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        events: Vec<String>,
        /// Values that are not a model to relaunch on.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ignore: Vec<String>,
    },
}

impl ModelReportSource {
    /// Whether the source is read on a report of `event`.
    pub fn applies_to(&self, event: &str) -> bool {
        let events = match self {
            Self::HookField { events, .. } | Self::Usage { events, .. } => events,
        };
        events.is_empty() || events.iter().any(|named| named == event)
    }

    /// `model` when it is a model to relaunch on: non-blank and not one of the source's `ignore`.
    pub fn accepts<'m>(&self, model: &'m str) -> Option<&'m str> {
        let ignore = match self {
            Self::HookField { ignore, .. } | Self::Usage { ignore, .. } => ignore,
        };
        let model = model.trim();
        (!model.is_empty() && !ignore.iter().any(|ignored| ignored == model)).then_some(model)
    }
}

/// Why `sources` cannot be read: a field-less hook source, or a usage source on an agent that
/// declares no `usage` binding.
pub(crate) fn validate_model_report(
    sources: &[ModelReportSource],
    has_usage: bool,
) -> Result<(), String> {
    for source in sources {
        match source {
            ModelReportSource::HookField { field, .. } if field.trim().is_empty() => {
                return Err("a hook_field source names no field".to_string());
            }
            ModelReportSource::Usage { .. } if !has_usage => {
                return Err(
                    "a usage source reads the agent's usage binding, which it does not declare \
                     (coding_agent.usage)"
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    Ok(())
}
