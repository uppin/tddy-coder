//! The compiler's `--message-format json` output, read as far as a tidy needs it.

use std::collections::BTreeMap;

use serde_json::Value;

/// One `compiler-message`: where it points, what it says, and the edits it offers.
pub(super) struct Diagnostic {
    pub code: Option<String>,
    pub level: String,
    pub message: String,
    /// The first primary span: `(file, line)`. `None` for a summary line such as "1 warning
    /// emitted".
    pub location: Option<(String, u64)>,
    /// Every primary span — one per thing the message is about.
    pub primaries: Vec<Span>,
    /// The machine-applicable replacements its children suggest.
    pub fixes: Vec<Fix>,
    /// How many compilation units built the target this message came from: a library checked with
    /// `--all-targets` is built twice, once as itself and once with its tests, and each unit says
    /// every warning of the code they share. One when the message names no target.
    pub units: usize,
}

/// Bytes `start..end` of `file`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Span {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

/// Replace `start..end` of `file` with `replacement`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Fix {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

impl Fix {
    /// Whether the edit's range lies over `span` of the same file.
    pub fn covers(&self, span: &Span) -> bool {
        self.file == span.file && self.start <= span.start && span.end <= self.end
    }
}

/// Every diagnostic in a check's stdout. Lines that are not compiler messages — artifacts, the
/// build summary — are not diagnostics and are skipped.
pub(super) fn parse(stdout: &str) -> Vec<Diagnostic> {
    let lines: Vec<Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    let units = units_per_target(&lines);
    lines
        .iter()
        .filter(|line| line["reason"] == "compiler-message")
        .map(|line| {
            let root = line["target"]["src_path"].as_str().unwrap_or_default();
            diagnostic_of(&line["message"], units.get(root).copied().unwrap_or(1))
        })
        .collect()
}

/// How many units finished per target source path: every unit ends with one `compiler-artifact`.
fn units_per_target(lines: &[Value]) -> BTreeMap<String, usize> {
    let mut units = BTreeMap::new();
    let artifacts = lines
        .iter()
        .filter(|line| line["reason"] == "compiler-artifact");
    for artifact in artifacts {
        let root = artifact["target"]["src_path"].as_str().unwrap_or_default();
        *units.entry(root.to_string()).or_default() += 1;
    }
    units
}

fn diagnostic_of(message: &Value, units: usize) -> Diagnostic {
    let spans = array_of(&message["spans"]);
    let primary: Vec<&Value> = spans
        .iter()
        .filter(|span| span["is_primary"] == true)
        .collect();
    Diagnostic {
        code: message["code"]["code"].as_str().map(str::to_string),
        level: text_of(&message["level"]),
        message: text_of(&message["message"]),
        location: primary.first().map(|span| location_of(span)),
        primaries: primary.iter().map(|span| span_of(span)).collect(),
        fixes: array_of(&message["children"])
            .iter()
            .flat_map(|child| array_of(&child["spans"]))
            .filter_map(fix_of)
            .collect(),
        units,
    }
}

fn array_of(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text_of(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_string()
}

fn location_of(span: &Value) -> (String, u64) {
    (
        text_of(&span["file_name"]),
        span["line_start"].as_u64().unwrap_or(0),
    )
}

fn span_of(span: &Value) -> Span {
    Span {
        file: text_of(&span["file_name"]),
        start: span["byte_start"].as_u64().unwrap_or(0) as usize,
        end: span["byte_end"].as_u64().unwrap_or(0) as usize,
    }
}

/// The edit a span carries, when rustc is sure of it — `MachineApplicable` and nothing weaker.
fn fix_of(span: &Value) -> Option<Fix> {
    if span["suggestion_applicability"] != "MachineApplicable" {
        return None;
    }
    Some(Fix {
        file: text_of(&span["file_name"]),
        start: span["byte_start"].as_u64()? as usize,
        end: span["byte_end"].as_u64()? as usize,
        replacement: span["suggested_replacement"].as_str()?.to_string(),
    })
}
