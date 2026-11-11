//! The binding model: what a spec's `usage:` section may say, as plain serde data.

use serde::{Deserialize, Serialize};

use crate::tree::{FILE_VARIABLES, MAIN_VARIABLES};
use crate::{FieldPath, PathTemplate};

/// An agent's usage binding: every source its usage is read from, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageSection {
    pub sources: Vec<UsageSource>,
}

/// One source of usage, tagged by its format. A format word this build does not implement is
/// refused when the spec loads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum UsageSource {
    /// Newline-delimited JSON files, one record per line.
    Jsonl(JsonlSource),
}

/// A `jsonl` source: the conversation's own files, how its children are found, and how a line is
/// read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonlSource {
    pub main: MainSpec,
    /// How a conversation's children are found — applied to each conversation found, recursively.
    /// `None` for an agent whose files hold no nested conversations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<ChildrenSpec>,
    pub record: RecordSpec,
}

/// The conversation's own file(s): a glob template; every match is merged into one conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainSpec {
    pub path: PathTemplate,
}

/// A conversation's children, located relative to one of its files (`{dir}`, `{stem}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChildrenSpec {
    /// A glob template over the parent file's `{dir}` and `{stem}`.
    pub path: PathTemplate,
    /// The child's conversation id, over the child file's `{dir}` and `{stem}`.
    pub id: PathTemplate,
    /// The child's display name. `None` names it after its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<NameSpec>,
}

/// A child's display name, read from a field of a sidecar JSON file beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameSpec {
    /// The sidecar's path, over the child file's `{dir}` and `{stem}`.
    pub sidecar: PathTemplate,
    pub field: FieldPath,
    /// The name used when the sidecar or its field is missing.
    pub default: String,
}

/// How each line of a conversation's files is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSpec {
    /// Every predicate must hold for a line to count.
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    pub where_: Vec<FieldEq>,
    /// Lines sharing this field's value are one record; the last such line wins. A line without
    /// the field is a record of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_by: Option<FieldPath>,
    #[serde(default)]
    pub aggregate: Aggregate,
    /// The model: the last value seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<FieldPath>,
    pub tokens: CounterMap,
    /// The reported cost in US dollars, aggregated like the counters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<FieldPath>,
}

/// A line counts only when the value at `field` is the string `equals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldEq {
    pub field: FieldPath,
    pub equals: String,
}

/// How the kept records of one conversation fold into its totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregate {
    /// Each counter is the sum over the kept records.
    #[default]
    Sum,
}

/// Where each counter is read from. The counters are disjoint: `input` is fresh input only and
/// `output` excludes reasoning, so a conversation's total is their plain sum. A counter left out
/// is zero.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterMap {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<FieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<FieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<FieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<FieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<FieldPath>,
}

/// Why `section` cannot be evaluated, in words a spec author can act on — the checks serde cannot
/// make: every template names only the variables its position binds, and there is at least one
/// source.
pub fn validate_usage_section(section: &UsageSection) -> Result<(), String> {
    if section.sources.is_empty() {
        return Err("sources is empty: name at least one source".to_string());
    }
    for source in &section.sources {
        let UsageSource::Jsonl(jsonl) = source;
        check_variables("main.path", &jsonl.main.path, MAIN_VARIABLES)?;
        if let Some(children) = &jsonl.children {
            check_variables("children.path", &children.path, FILE_VARIABLES)?;
            check_variables("children.id", &children.id, FILE_VARIABLES)?;
            if let Some(name) = &children.name {
                check_variables("children.name.sidecar", &name.sidecar, FILE_VARIABLES)?;
            }
        }
    }
    Ok(())
}

/// `template` names only variables from `allowed`.
fn check_variables(
    position: &str,
    template: &PathTemplate,
    allowed: &[&str],
) -> Result<(), String> {
    match template
        .variables()
        .into_iter()
        .find(|v| !allowed.contains(v))
    {
        Some(unknown) => Err(format!(
            "{position} '{}' names unknown variable '{{{unknown}}}' (known: {})",
            template.as_str(),
            allowed
                .iter()
                .map(|v| format!("{{{v}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        None => Ok(()),
    }
}
