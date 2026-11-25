//! `coding_agent.activity`: where a session's activity comes from — the user's prompts and the
//! agent's replies (`messages`), and its tool calls (`tool_calls`) — and, for an agent whose own
//! transcript file is a source, how that file's lines map to activity.
//!
//! ```yaml
//! activity:
//!   messages: transcript        # hooks | transcript | both; hooks when absent
//!   tool_calls: hooks           # hooks | transcript | both; hooks when absent
//!   hook_messages:              # the hook events that carry spoken text
//!     - { event: UserPromptSubmit, field: prompt, role: user }
//!   transcript:
//!     path: "{agent_home}/.cursor/projects/{worktree_slug}/agent-transcripts/{conversation_id}/{conversation_id}.jsonl"
//!     lines:                    # the first rule whose `where` holds for a line maps it
//!       - where: [{ field: role, equals: assistant }]
//!         parts: message.content    # each element of this array is matched against `emit`
//!         emit:
//!           - { kind: agent_message, where: [{ field: type, equals: text }], text: text }
//!           - { kind: tool_use, where: [{ field: type, equals: tool_use }], name: name, input: input }
//!       - where: [{ field: type, equals: turn_ended }]
//!         emit: [{ kind: turn_end }]
//! ```
//!
//! The transcript's shape is the spec's alone: [`TranscriptSource::map_line`] is one generic mapper
//! over the declared rules, reusing the usage binding's `FieldPath` and `where` predicates.

use serde::{Deserialize, Serialize};
use tddy_usage::{FieldEq, FieldPath, PathTemplate, TemplateError};

/// Where one kind of activity is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    /// The agent's hooks, as reported to the daemon.
    #[default]
    Hooks,
    /// The agent's own transcript file, followed as it grows.
    Transcript,
    /// Both, an entry seen in both recorded once.
    Both,
}

impl ActivitySource {
    /// Whether the agent's hooks are read.
    pub fn reads_hooks(self) -> bool {
        matches!(self, Self::Hooks | Self::Both)
    }

    /// Whether the agent's transcript is read.
    pub fn reads_transcript(self) -> bool {
        matches!(self, Self::Transcript | Self::Both)
    }
}

/// Who spoke a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpokenRole {
    /// The user's prompt.
    User,
    /// The agent's reply.
    Agent,
}

/// A hook event that carries what was said: the string at its top-level `field`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookMessageSource {
    pub event: String,
    pub field: String,
    pub role: SpokenRole,
}

/// The `activity` slot. Its default reads everything from hooks and names no spoken message.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivitySlot {
    #[serde(default)]
    pub messages: ActivitySource,
    #[serde(default)]
    pub tool_calls: ActivitySource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hook_messages: Vec<HookMessageSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<TranscriptSource>,
}

impl ActivitySlot {
    /// Whether the slot is the default: nothing declared.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// The transcript to follow, when either kind of activity is read from it.
    pub fn followed_transcript(&self) -> Option<&TranscriptSource> {
        (self.messages.reads_transcript() || self.tool_calls.reads_transcript())
            .then_some(self.transcript.as_ref())
            .flatten()
    }

    /// The text the hook event `event` carries in `fields`, and who said it, when the slot reads
    /// messages from hooks and names that event.
    pub fn spoken_in_hook<'f>(
        &self,
        event: &str,
        fields: &'f std::collections::HashMap<String, String>,
    ) -> Option<(SpokenRole, &'f str)> {
        if !self.messages.reads_hooks() {
            return None;
        }
        self.hook_messages
            .iter()
            .filter(|source| source.event == event)
            .find_map(|source| {
                fields
                    .get(&source.field)
                    .map(String::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| (source.role, text))
            })
    }
}

/// The agent's transcript: where it is, and how its lines map to activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptSource {
    /// The file, over [`TRANSCRIPT_PATH_VARIABLES`].
    pub path: PathTemplate,
    /// The rules, tried in order: the first whose `where` holds for a line maps it.
    pub lines: Vec<LineRule>,
}

/// The variables a transcript path may name.
pub const TRANSCRIPT_PATH_VARIABLES: &[&str] =
    &["agent_home", "conversation_id", "worktree", "worktree_slug"];

/// What a transcript path is rendered with — every value as the machine the agent runs on sees
/// it: on a target, the target's home and the target's worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptPathVars {
    /// The agent's `HOME`.
    pub agent_home: String,
    /// The session's own conversation (Cursor's chat id).
    pub conversation_id: String,
    /// The session's worktree; `{worktree_slug}` is derived from it.
    pub worktree: String,
}

/// How one kind of line maps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineRule {
    /// What a line must hold for the rule to map it; none for every line.
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub where_: Vec<FieldEq>,
    /// An array of the line whose elements are each matched against `emit`; absent to match the
    /// line itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<FieldPath>,
    /// The entries the line — or each part — maps to: the first whose `where` holds.
    pub emit: Vec<EmitRule>,
}

/// The kind of entry an [`EmitRule`] makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    UserMessage,
    AgentMessage,
    ToolUse,
    TurnEnd,
}

/// One entry a matched line or part makes, and where its values are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitRule {
    pub kind: EntryKind,
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub where_: Vec<FieldEq>,
    /// A message's text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<FieldPath>,
    /// Keep only the trimmed inside of `<tag>…</tag>` when the text holds one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unwrap_tag: Option<String>,
    /// A tool call's tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<FieldPath>,
    /// A tool call's input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<FieldPath>,
    /// A tool call's own id, for an agent whose transcript has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<FieldPath>,
}

/// One thing a transcript line says.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEntry {
    UserMessage {
        text: String,
    },
    AgentMessage {
        text: String,
    },
    ToolUse {
        name: String,
        input: serde_json::Value,
        id: Option<String>,
    },
    TurnEnd,
}

fn holds(predicates: &[FieldEq], value: &serde_json::Value) -> bool {
    predicates
        .iter()
        .all(|predicate| predicate.holds_for(value))
}

fn string_at(path: &FieldPath, value: &serde_json::Value) -> Option<String> {
    path.lookup(value)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

/// The trimmed inside of the first `<tag>…</tag>` in `text`, or `text` unchanged without one.
fn unwrapped(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    text.find(&open)
        .and_then(|start| {
            let inner = start + open.len();
            text[inner..]
                .find(&close)
                .map(|end| text[inner..inner + end].trim().to_string())
        })
        .unwrap_or_else(|| text.to_string())
}

impl EmitRule {
    fn entry(&self, value: &serde_json::Value) -> Option<TranscriptEntry> {
        let text = || {
            let text = string_at(self.text.as_ref()?, value)?;
            let text = match &self.unwrap_tag {
                Some(tag) => unwrapped(&text, tag),
                None => text,
            };
            Some(text).filter(|text| !text.trim().is_empty())
        };
        match self.kind {
            EntryKind::UserMessage => Some(TranscriptEntry::UserMessage { text: text()? }),
            EntryKind::AgentMessage => Some(TranscriptEntry::AgentMessage { text: text()? }),
            EntryKind::ToolUse => Some(TranscriptEntry::ToolUse {
                name: string_at(self.name.as_ref()?, value)?,
                input: self
                    .input
                    .as_ref()
                    .and_then(|path| path.lookup(value))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                id: self.id.as_ref().and_then(|path| string_at(path, value)),
            }),
            EntryKind::TurnEnd => Some(TranscriptEntry::TurnEnd),
        }
    }
}

impl LineRule {
    fn entries(&self, line: &serde_json::Value) -> Vec<TranscriptEntry> {
        let first_emitted = |value: &serde_json::Value| {
            self.emit
                .iter()
                .find(|rule| holds(&rule.where_, value))
                .and_then(|rule| rule.entry(value))
        };
        match &self.parts {
            Some(parts) => parts
                .lookup(line)
                .and_then(serde_json::Value::as_array)
                .map(|parts| parts.iter().filter_map(first_emitted).collect())
                .unwrap_or_default(),
            None => first_emitted(line).into_iter().collect(),
        }
    }
}

impl TranscriptSource {
    /// What `line` says, in its own order: empty for a line no rule maps.
    pub fn map_line(&self, line: &serde_json::Value) -> Vec<TranscriptEntry> {
        self.lines
            .iter()
            .find(|rule| holds(&rule.where_, line))
            .map(|rule| rule.entries(line))
            .unwrap_or_default()
    }

    /// The transcript's path, rendered with `vars`.
    pub fn render_path(&self, vars: &TranscriptPathVars) -> Result<String, TemplateError> {
        self.path
            .render(TRANSCRIPT_PATH_VARIABLES, &|name| match name {
                "agent_home" => Some(vars.agent_home.clone()),
                "conversation_id" => Some(vars.conversation_id.clone()).filter(|id| !id.is_empty()),
                "worktree" => Some(vars.worktree.clone()),
                "worktree_slug" => Some(worktree_slug(&vars.worktree)),
                _ => None,
            })
    }
}

/// `path` with every run of characters other than ASCII letters and digits made one `-`, and none
/// at either end — how Cursor names a workspace's directory under `~/.cursor/projects`.
pub fn worktree_slug(path: &str) -> String {
    let mut slug = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Why `slot` cannot be read: a source naming a transcript it does not declare, a path naming an
/// unknown variable, a hook message naming no field, or an entry missing the value its kind needs.
pub(crate) fn validate_activity_slot(slot: &ActivitySlot) -> Result<(), String> {
    for (kind, source) in [("messages", slot.messages), ("tool_calls", slot.tool_calls)] {
        if source.reads_transcript() && slot.transcript.is_none() {
            return Err(format!(
                "{kind} are read from a transcript, and the slot declares no `transcript`"
            ));
        }
    }
    for source in &slot.hook_messages {
        if source.event.trim().is_empty() || source.field.trim().is_empty() {
            return Err("a hook message names no event or no field".to_string());
        }
    }
    let Some(transcript) = &slot.transcript else {
        return Ok(());
    };
    for variable in transcript.path.variables() {
        if !TRANSCRIPT_PATH_VARIABLES.contains(&variable) {
            return Err(format!(
                "transcript.path '{}' names unknown variable '{{{variable}}}'",
                transcript.path.as_str()
            ));
        }
    }
    for rule in transcript.lines.iter().flat_map(|line| &line.emit) {
        let missing = match rule.kind {
            EntryKind::UserMessage | EntryKind::AgentMessage if rule.text.is_none() => Some("text"),
            EntryKind::ToolUse if rule.name.is_none() => Some("name"),
            _ => None,
        };
        if let Some(missing) = missing {
            let kind = serde_json::to_value(rule.kind)
                .ok()
                .and_then(|kind| kind.as_str().map(str::to_string))
                .unwrap_or_default();
            return Err(format!(
                "transcript: an entry of kind {kind} names no {missing}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_tag_is_unwrapped_to_its_trimmed_inside() {
        assert_eq!(
            unwrapped(
                "<timestamp>Tuesday</timestamp>\n<user_query>\nfix it\n</user_query>",
                "user_query"
            ),
            "fix it"
        );
    }

    #[test]
    fn text_without_the_tag_is_kept_as_it_is() {
        assert_eq!(unwrapped("just text", "user_query"), "just text");
    }

    #[test]
    fn hook_messages_are_not_read_when_messages_come_from_the_transcript() {
        // Given a slot reading messages from its transcript that still names a hook message
        let slot = ActivitySlot {
            messages: ActivitySource::Transcript,
            hook_messages: vec![HookMessageSource {
                event: "Stop".to_string(),
                field: "last_assistant_message".to_string(),
                role: SpokenRole::Agent,
            }],
            ..ActivitySlot::default()
        };
        let fields = std::collections::HashMap::from([(
            "last_assistant_message".to_string(),
            "done".to_string(),
        )]);

        // Then
        assert_eq!(slot.spoken_in_hook("Stop", &fields), None);
    }
}
