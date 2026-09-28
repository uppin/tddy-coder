//! A model-issued tool call, checked against the schema the model was actually given.
//!
//! Session 01a0e200 spent eight of twenty-four calls on arguments carrying a stray trailing quote
//! — `{"path": "packages/tddy-web/src/lib/terminalGridMeasure.ts\""}`. Five were globs, which
//! matched nothing and came back `{"paths": []}` with no error at all: a malformed argument and a
//! pattern that legitimately matches nothing were the same answer. The three reads came back
//! `file not found`, which is the same answer a real missing file gives, so the agent's correct
//! reading was "wrong path, try another" and it retried the identical broken path twice.
//!
//! The rule: a call the schema rejects never reaches the codebase, and the reason names the
//! argument, the problem and the value.
//!
//! The schema checked against is the **advertised** one — [`discovery_tool_definitions`],
//! [`mutation_tool_definitions`] and [`engine_tool_definitions`] — rather than a second list
//! written here. A validator with its own idea of what a tool takes would fault calls that
//! honour what the model was told, which is worse than no validator at all.

use crate::openai::{
    discovery_tool_definitions, engine_tool_definitions, mutation_tool_definitions,
};

/// What is wrong with one argument of a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentProblem {
    /// The schema requires it and the call did not supply it.
    Missing,
    /// Supplied, but blank. Its own defect rather than [`Self::Missing`]: reporting a blank
    /// argument as missing sends the model looking for one it did supply.
    Empty,
    /// Not an argument this tool accepts — a typo, today silently dropped, so a model asking for
    /// a window gets a whole file and no hint that its spelling was wrong.
    Unknown,
    /// The schema asks for one JSON type and the call supplied another. Dispatched today as an
    /// empty string, because `as_str()` returns `None` and the call site substitutes `""`.
    WrongType { expected: String },
    /// A leading or trailing quote character on an argument that names files — the defect from
    /// session 01a0e200.
    SurroundingQuote,
    /// An anchor with no content in it — `"\n\n"`, `" "`, `""` — which cannot single out one
    /// place in a file, so the edit it anchors was never going to land.
    ///
    /// Session 01a0e285 sent `old_string: "\n\n"` four times and was answered `old_string
    /// matches 51 times (must be unique)` each time. That answer is accurate and the agent
    /// repeated the call anyway, so the value here is not a better message after the fact: it is
    /// refusing the value up front, next to the other argument faults, rather than three layers
    /// down as a uniqueness count that reads like bad luck.
    NotAnAnchor,
    /// An integer outside the range the schema spells — past its `maximum` or below its
    /// `minimum`. Its own defect rather than [`Self::WrongType`]: the value is the right shape,
    /// just one the advertised bound refuses, and naming the bound lets the model re-ask within
    /// it instead of probing for where the edge was.
    OutOfRange { allowed: String },
}

impl ArgumentProblem {
    /// What the model is told about this problem, in the words it has to act on.
    fn describe(&self) -> String {
        match self {
            Self::Missing => {
                "this tool requires the argument and the call did not supply it".to_string()
            }
            Self::Empty => "the argument is present but blank".to_string(),
            Self::Unknown => "this tool has no such argument — check the spelling against the \
                              tool's schema"
                .to_string(),
            Self::WrongType { expected } => {
                format!("the schema asks for a {expected} and this is not one")
            }
            Self::SurroundingQuote => {
                "the value starts or ends with a quote character, so it is a quoted literal \
                 rather than the path itself — send the path without the quotes"
                    .to_string()
            }
            Self::NotAnAnchor => {
                "the value is nothing but whitespace, so it names no place in particular — \
                 anchor the edit on the surrounding code, with the whitespace you mean to change \
                 inside it"
                    .to_string()
            }
            Self::OutOfRange { allowed } => {
                format!("the value is outside the range the schema allows ({allowed})")
            }
        }
    }
}

/// One argument of a tool call that does not match the schema, and the value that was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentViolation {
    pub argument: String,
    pub problem: ArgumentProblem,
    /// The offending value as the call spelled it, so a reader can see the defect. `None` for an
    /// argument that was never supplied — there is nothing to quote back.
    pub value: Option<String>,
}

impl ArgumentViolation {
    fn new(argument: &str, problem: ArgumentProblem, value: Option<String>) -> Self {
        Self {
            argument: argument.to_string(),
            problem,
            value,
        }
    }

    /// This violation as one entry of the breakdown carried back to the model.
    fn as_json(&self) -> serde_json::Value {
        let mut entry = serde_json::json!({
            "argument": self.argument,
            "problem": self.problem.describe(),
        });
        if let (Some(object), Some(value)) = (entry.as_object_mut(), self.value.as_ref()) {
            object.insert("value".to_string(), serde_json::json!(value));
        }
        entry
    }
}

/// The arguments whose value names a file, and which therefore must not carry a surrounding
/// quote.
///
/// `pattern` is path-like on `GLOB` and **not** on `GREP`: a glob pattern is a path with
/// wildcards in it, while a regex may legally end in a quote — `const NAME = "` is a search a
/// reader would make, and faulting it would make `GREP` unusable for searching quoted source.
/// Free text (`contents`, `command`, `query`, `old_string`, `new_string`) is never path-like: a
/// file body legitimately ends in whatever it ends in.
fn is_path_like(tool: &str, argument: &str) -> bool {
    argument == "path" || (tool == "GLOB" && argument == "pattern")
}

/// The arguments whose value is a document rather than a target, and for which blank is a
/// legitimate request — writing an empty file, or replacing a string with nothing.
const FREE_TEXT_ARGUMENTS: &[&str] = &["contents", "old_string", "new_string"];

/// The argument whose job is to *locate* an edit rather than to carry text into one.
///
/// Only `old_string`: `new_string` is the replacement itself, and whitespace or nothing at all
/// there is an ordinary deletion. The rule is about an anchor with no content in it, never about
/// an anchor that *contains* whitespace — a real edit routinely spans blank lines, and faulting
/// those would make the tool unusable for exactly the tidy-up this incident was about.
fn is_anchor(argument: &str) -> bool {
    argument == "old_string"
}

/// Whether `value` matches the JSON type the schema spells, or `None` when the schema names a
/// type this check does not model — in which case nothing is asserted rather than something
/// guessed.
fn matches_declared_type(declared: &str, value: &serde_json::Value) -> Option<bool> {
    match declared {
        "string" => Some(value.is_string()),
        "integer" => Some(value.is_i64() || value.is_u64()),
        "number" => Some(value.is_number()),
        "boolean" => Some(value.is_boolean()),
        "array" => Some(value.is_array()),
        "object" => Some(value.is_object()),
        _ => None,
    }
}

/// Whether `value` is a number outside the range `declared` spells with `minimum`/`maximum`, or
/// `None` when the schema names no bound (or the value is not a number — a shape fault, which
/// [`ArgumentProblem::WrongType`] already names).
fn out_of_range(
    declared: &serde_json::Value,
    value: &serde_json::Value,
) -> Option<ArgumentProblem> {
    let minimum = declared["minimum"].as_i64();
    let maximum = declared["maximum"].as_i64();
    if minimum.is_none() && maximum.is_none() {
        return None;
    }
    // Widest signed room, so a value past `i64` still reads as past any `maximum` the schema
    // names rather than slipping the check.
    let number = value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from))?;
    let below = minimum.is_some_and(|min| number < i128::from(min));
    let above = maximum.is_some_and(|max| number > i128::from(max));
    (below || above).then(|| ArgumentProblem::OutOfRange {
        allowed: allowed_range(minimum, maximum),
    })
}

/// The range as the model is told it: `0 to 50`, or the one-sided spellings.
fn allowed_range(minimum: Option<i64>, maximum: Option<i64>) -> String {
    match (minimum, maximum) {
        (Some(min), Some(max)) => format!("{min} to {max}"),
        (Some(min), None) => format!("at least {min}"),
        (None, Some(max)) => format!("at most {max}"),
        // Only reached when a bound exists — `out_of_range` returns early otherwise.
        (None, None) => String::new(),
    }
}

/// The value as the breakdown quotes it back: a string verbatim, anything else as its JSON.
fn quoted_back(value: &serde_json::Value) -> String {
    match value.as_str() {
        Some(text) => text.to_string(),
        None => value.to_string(),
    }
}

/// What is wrong with one declared argument, or `None` when nothing is.
///
/// At most one problem per argument, in the order a reader would find them: absent, then the
/// wrong shape, then outside the declared range, then blank, then unusable as an anchor, then
/// quoted. A value has only one defect worth naming, and listing two for the same argument would
/// read as two arguments to fix.
fn fault_in(
    tool: &str,
    argument: &str,
    declared: Option<&serde_json::Value>,
    supplied: Option<&serde_json::Value>,
    required: bool,
) -> Option<ArgumentProblem> {
    // JSON's `null` is how a caller spells an absent value, so it is read as one rather than as a
    // value of the wrong type.
    let supplied = supplied.filter(|value| !value.is_null());
    let Some(value) = supplied else {
        return required.then_some(ArgumentProblem::Missing);
    };

    if let Some(declared) = declared {
        if let Some(declared_type) = declared["type"].as_str() {
            if matches_declared_type(declared_type, value) == Some(false) {
                return Some(ArgumentProblem::WrongType {
                    expected: declared_type.to_string(),
                });
            }
        }
        if let Some(problem) = out_of_range(declared, value) {
            return Some(problem);
        }
    }

    let text = value.as_str()?;
    if required && !FREE_TEXT_ARGUMENTS.contains(&argument) && text.trim().is_empty() {
        return Some(ArgumentProblem::Empty);
    }
    if is_anchor(argument) && text.trim().is_empty() {
        return Some(ArgumentProblem::NotAnAnchor);
    }
    let quoted = |c: char| c == '"' || c == '\'';
    if is_path_like(tool, argument) && (text.starts_with(quoted) || text.ends_with(quoted)) {
        return Some(ArgumentProblem::SurroundingQuote);
    }
    None
}

/// The advertised schema for `tool`, or `None` for a name no definition list carries.
fn advertised_schema(tool: &str) -> Option<serde_json::Value> {
    discovery_tool_definitions()
        .into_iter()
        .chain(mutation_tool_definitions())
        .chain(engine_tool_definitions())
        .find(|definition| definition.function.name == tool)
        .map(|definition| definition.function.parameters)
}

/// Every way `args` fails the schema `tool` was advertised with — the whole breakdown, not the
/// first fault.
///
/// One round trip per defect would cost a 32k agent its whole budget, so a call with two
/// problems is answered with two problems. The declared arguments come first, in schema order,
/// then the keys the schema does not carry.
///
/// A tool no definition list names is not faulted here: dispatch refuses an unknown tool by name
/// already, and inventing a schema for one would fault a call against a contract nobody
/// advertised.
pub fn validate_tool_arguments(tool: &str, args: &serde_json::Value) -> Vec<ArgumentViolation> {
    let Some(schema) = advertised_schema(tool) else {
        return Vec::new();
    };
    let Some(properties) = schema["properties"].as_object() else {
        return Vec::new();
    };
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|names| names.iter().filter_map(|name| name.as_str()).collect())
        .unwrap_or_default();

    let mut violations = Vec::new();
    for (argument, declared) in properties {
        let supplied = args.get(argument);
        let problem = fault_in(
            tool,
            argument,
            Some(declared),
            supplied,
            required.contains(&argument.as_str()),
        );
        if let Some(problem) = problem {
            violations.push(ArgumentViolation::new(
                argument,
                problem,
                supplied.map(quoted_back),
            ));
        }
    }

    for (argument, value) in args.as_object().into_iter().flatten() {
        if !properties.contains_key(argument) {
            violations.push(ArgumentViolation::new(
                argument,
                ArgumentProblem::Unknown,
                Some(quoted_back(value)),
            ));
        }
    }
    violations
}

/// The `tool`-role message body for a call the schema rejected.
///
/// Built with `serde_json` rather than assembled as text, for the reason every tool result is: a
/// rejected value is arbitrary — it is the thing that was wrong — and one carrying a quote or a
/// newline would otherwise produce a payload that is not valid JSON on the one turn the model
/// most needs to read it.
///
/// It never says "file not found". That reading is what sent session 01a0e200 round the same
/// broken path twice: nothing was looked for, so nothing was missing.
pub(crate) fn rejection_payload(tool: &str, violations: &[ArgumentViolation]) -> serde_json::Value {
    let named: Vec<&str> = violations
        .iter()
        .map(|violation| violation.argument.as_str())
        .collect();
    serde_json::json!({
        "error": format!(
            "{tool} was not dispatched: its arguments do not match the schema this tool was \
             advertised with. Nothing was searched for and nothing is missing — fix the \
             arguments named below and call again. Arguments at fault: {}.",
            named.join(", ")
        ),
        "validation": violations
            .iter()
            .map(ArgumentViolation::as_json)
            .collect::<Vec<_>>(),
    })
}

/// The one-line reason a rejected call produced no result, for the turn tally and the logs.
pub(crate) fn rejection_reason(tool: &str, violations: &[ArgumentViolation]) -> String {
    let faults: Vec<String> = violations
        .iter()
        .map(|violation| format!("{}: {}", violation.argument, violation.problem.describe()))
        .collect();
    format!(
        "{tool} was not dispatched — {} argument(s) do not match its advertised schema ({})",
        violations.len(),
        faults.join("; ")
    )
}
