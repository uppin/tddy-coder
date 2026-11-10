//! The binding model: what a spec's `usage:` section may say, as plain serde data.

use serde::{Deserialize, Serialize};

use crate::tree::{FILE_VARIABLES, MAIN_VARIABLES};
use crate::{CounterExpr, FieldPath, PathTemplate};

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
    /// One row per conversation in a SQLite table, opened read-only (`#usage-binding 5/5`).
    Sqlite(SqliteSource),
}

/// A `sqlite` source: the database, the table holding one row per conversation, and which columns
/// say what. Every table and column name is checked to be a plain identifier when the spec loads,
/// so a binding cannot carry SQL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqliteSource {
    /// The database file, over the session variables.
    pub path: PathTemplate,
    pub table: String,
    /// The column holding a conversation's id.
    pub id: String,
    /// The column holding the id of the conversation a row is nested under; rows whose parent is a
    /// conversation's id are its children, recursively. `None` for a table with no nesting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub root: RootMatch,
    /// Counter expressions over the row's column names.
    pub tokens: CounterMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<FieldPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<FieldPath>,
    /// The column naming a child conversation's agent; a child without one is named after its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<FieldPath>,
}

/// The main conversation: the root row (no parent) whose `column` equals the rendered `equals`,
/// the newest by `newest_by` when several do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootMatch {
    pub column: String,
    pub equals: PathTemplate,
    pub newest_by: String,
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
    /// Where a file's conversation id and parent id are read from, when the path does not say: the
    /// first line holding the `id` field (`#usage-binding 4/5`). `None` takes the id from the path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentitySpec>,
    /// Children found by the parent id their own files declare, rather than by a path relative to
    /// the parent: every file the main glob matches whose `identity.parent` is a conversation's id
    /// is its child, recursively (`#usage-binding 4/5`). Requires `identity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_by: Option<ChildrenBy>,
    pub record: RecordSpec,
}

/// A file's conversation id, and the id of the conversation it is nested under, as fields of its
/// header line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentitySpec {
    pub id: FieldPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<FieldPath>,
}

/// How children are found when not by path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildrenBy {
    /// By `identity.parent`.
    ParentField,
}

/// The main conversation is the newest file (by modification time) whose header `field` equals
/// the rendered `equals` and which declares no parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainMatch {
    pub field: FieldPath,
    pub equals: PathTemplate,
}

/// The conversation's own file(s): a glob template; every match is merged into one conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MainSpec {
    pub path: PathTemplate,
    /// Pick one file among the matches by a header field, rather than merging every match
    /// (`#usage-binding 4/5`).
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_: Option<MainMatch>,
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
    /// Each counter is its value on the last kept record that has it — for a source whose counters
    /// are already cumulative (`#usage-binding 4/5`).
    Last,
}

/// Where each counter is read from — a field path, or paths joined by `+` / `-`
/// ([`CounterExpr`]). The counters are disjoint: `input` is fresh input only and
/// `output` excludes reasoning, so a conversation's total is their plain sum. A counter left out
/// is zero.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterMap {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<CounterExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<CounterExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<CounterExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<CounterExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<CounterExpr>,
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
        check_source(jsonl)?;
    }
    check_counters_unowned_twice(section)
}

/// One source's templates name only the variables their position binds, and its identity-dependent
/// features come with an identity.
fn check_source(jsonl: &JsonlSource) -> Result<(), String> {
    check_variables("main.path", &jsonl.main.path, MAIN_VARIABLES)?;
    if let Some(m) = &jsonl.main.match_ {
        check_variables("main.match.equals", &m.equals, MAIN_VARIABLES)?;
    }
    check_identity(jsonl)?;
    if let Some(children) = &jsonl.children {
        check_variables("children.path", &children.path, FILE_VARIABLES)?;
        check_variables("children.id", &children.id, FILE_VARIABLES)?;
        if let Some(name) = &children.name {
            check_variables("children.name.sidecar", &name.sidecar, FILE_VARIABLES)?;
        }
    }
    Ok(())
}

/// `children_by` and `main.match` read a file's header identity, which path-located `children`
/// replace.
fn check_identity(jsonl: &JsonlSource) -> Result<(), String> {
    if jsonl.identity.is_none() {
        for (position, used) in [
            ("children_by", jsonl.children_by.is_some()),
            ("main.match", jsonl.main.match_.is_some()),
        ] {
            if used {
                return Err(format!(
                    "{position} needs an identity: say which fields hold a file's id and parent"
                ));
            }
        }
    } else if jsonl.children.is_some() {
        return Err(
            "children locates by path, which an identity replaces: use children_by instead"
                .to_string(),
        );
    }
    Ok(())
}

/// No counter (nor the cost) is mapped by two sources: ownership is the mapping, and the merge of
/// one conversation's rows relies on each counter having exactly one source.
fn check_counters_unowned_twice(section: &UsageSection) -> Result<(), String> {
    let mut owners: Vec<(&'static str, usize)> = Vec::new();
    for (index, source) in section.sources.iter().enumerate() {
        let UsageSource::Jsonl(jsonl) = source;
        let record = &jsonl.record;
        let tokens = &record.tokens;
        let mapped = [
            ("input", tokens.input.is_some()),
            ("cache_read", tokens.cache_read.is_some()),
            ("cache_write", tokens.cache_write.is_some()),
            ("output", tokens.output.is_some()),
            ("reasoning", tokens.reasoning.is_some()),
            ("cost", record.cost.is_some()),
        ];
        for (counter, is_mapped) in mapped {
            if !is_mapped {
                continue;
            }
            if let Some((_, first)) = owners.iter().find(|(c, _)| *c == counter) {
                return Err(format!(
                    "sources[{first}] and sources[{index}] both map the counter '{counter}': \
                     a counter is mapped by one source"
                ));
            }
            owners.push((counter, index));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn section(source: serde_json::Value) -> UsageSection {
        serde_json::from_value(serde_json::json!({ "sources": [source] })).expect("a section")
    }

    #[test]
    fn an_identity_beside_a_path_located_children_is_refused() {
        let section = section(serde_json::json!({
            "format": "jsonl",
            "main": { "path": "{session_dir}/*.jsonl" },
            "identity": { "id": "id" },
            "children": { "path": "{dir}/*.jsonl", "id": "{stem}" },
            "record": { "tokens": { "input": "n" } }
        }));

        let refusal = validate_usage_section(&section).unwrap_err();

        assert!(refusal.contains("children_by"), "{refusal}");
    }

    #[test]
    fn a_counter_mapped_by_two_sources_is_refused() {
        let source = serde_json::json!({
            "format": "jsonl",
            "main": { "path": "{session_dir}/*.jsonl" },
            "record": { "tokens": { "input": "n" } }
        });
        let section: UsageSection =
            serde_json::from_value(serde_json::json!({ "sources": [source.clone(), source] }))
                .expect("a section");

        let refusal = validate_usage_section(&section).unwrap_err();

        assert!(
            refusal.contains("both map the counter 'input'"),
            "{refusal}"
        );
    }
}
