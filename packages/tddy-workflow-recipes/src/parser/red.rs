//! The `red` phase's parser: `RedOutput` with its `RedTestInfo`, `SkeletonInfo`, `MarkerInfo` and
//! `MarkerResult` records, `parse_red_response` and `validate_red_marker_source_paths`.

use tddy_core::{classify_rust_source_path, ParseError, RustSourcePathKind};

/// Parsed red goal output.
#[derive(Debug, Clone)]
pub struct RedOutput {
    pub summary: String,
    pub tests: Vec<RedTestInfo>,
    pub skeletons: Vec<SkeletonInfo>,
    /// How to run the tests, derived from project (e.g. "cargo test", "npm test").
    pub test_command: Option<String>,
    /// Prerequisite actions before running tests. Use cheapest way: omit if test script already builds.
    pub prerequisite_actions: Option<String>,
    /// How to run a single or selected tests (e.g. "cargo test <name>", "pytest -k <pattern>").
    pub run_single_or_selected_tests: Option<String>,
    /// Logging markers added to skeleton code.
    #[allow(clippy::struct_excessive_bools)]
    pub markers: Vec<MarkerInfo>,
    /// Which markers were collected from test output.
    pub marker_results: Vec<MarkerResult>,
    /// Path to captured test output file.
    pub test_output_file: Option<String>,
    /// How to run tests sequentially.
    pub sequential_command: Option<String>,
    /// How to run tests with logging.
    pub logging_command: Option<String>,
    /// Metric reporting hooks.
    pub metric_hooks: Option<String>,
    /// Execution feedback options.
    pub feedback_options: Option<String>,
}

/// Logging marker definition (JSON format with scope data).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkerInfo {
    pub marker_id: String,
    pub test_name: String,
    pub scope: String,
    pub data: serde_json::Value,
    /// File where the marker was placed (production skeleton entry point), when provided.
    #[serde(default)]
    pub source_file: Option<String>,
}

/// Result of marker collection verification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkerResult {
    pub marker_id: String,
    pub test_name: String,
    pub scope: String,
    pub collected: bool,
    pub investigation: Option<String>,
}

/// Info about a single test created by the red goal.
#[derive(Debug, Clone)]
pub struct RedTestInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub status: String,
}

/// Info about a skeleton (trait, struct, method, function, module) created by the red goal.
#[derive(Debug, Clone)]
pub struct SkeletonInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
}

#[derive(serde::Deserialize)]
struct StructuredRed {
    pub(crate) goal: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) tests: Option<Vec<RedTestInfoDe>>,
    pub(crate) skeletons: Option<Vec<SkeletonInfoDe>>,
    pub(crate) test_command: Option<String>,
    pub(crate) prerequisite_actions: Option<String>,
    pub(crate) run_single_or_selected_tests: Option<String>,
    #[serde(default)]
    pub(crate) markers: Option<Vec<MarkerInfoDe>>,
    #[serde(default)]
    pub(crate) marker_results: Option<Vec<MarkerResultDe>>,
    #[serde(default)]
    pub(crate) test_output_file: Option<String>,
    #[serde(default)]
    pub(crate) sequential_command: Option<String>,
    #[serde(default)]
    pub(crate) logging_command: Option<String>,
    #[serde(default)]
    pub(crate) metric_hooks: Option<String>,
    #[serde(default)]
    pub(crate) feedback_options: Option<String>,
}

#[derive(serde::Deserialize)]
struct MarkerInfoDe {
    pub(crate) marker_id: String,
    pub(crate) test_name: String,
    pub(crate) scope: String,
    #[serde(default)]
    pub(crate) data: serde_json::Value,
    #[serde(default)]
    pub(crate) source_file: Option<String>,
}

#[derive(serde::Deserialize)]
struct MarkerResultDe {
    pub(crate) marker_id: String,
    pub(crate) test_name: String,
    pub(crate) scope: String,
    pub(crate) collected: bool,
    pub(crate) investigation: Option<String>,
}

#[derive(serde::Deserialize)]
struct RedTestInfoDe {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) status: String,
}

#[derive(serde::Deserialize)]
struct SkeletonInfoDe {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) kind: String,
}

/// Parse LLM red goal response. JSON must come from tddy-tools submit.
pub fn parse_red_response(s: &str) -> Result<RedOutput, ParseError> {
    log::info!(target: "tddy_workflow_recipes::parser", "parse_red_response: parsing red goal JSON");
    let s = s.trim();
    let parsed: StructuredRed = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("red") {
        return Err(ParseError::Malformed("goal is not red".into()));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    let tests = parsed
        .tests
        .unwrap_or_default()
        .into_iter()
        .map(|t| RedTestInfo {
            name: t.name,
            file: t.file,
            line: t.line,
            status: t.status,
        })
        .collect();
    let skeletons = parsed
        .skeletons
        .unwrap_or_default()
        .into_iter()
        .map(|s| SkeletonInfo {
            name: s.name,
            file: s.file,
            line: s.line,
            kind: s.kind,
        })
        .collect();
    let markers = parsed
        .markers
        .unwrap_or_default()
        .into_iter()
        .map(|m| MarkerInfo {
            marker_id: m.marker_id,
            test_name: m.test_name,
            scope: m.scope,
            data: m.data,
            source_file: m.source_file.filter(|s| !s.is_empty()),
        })
        .collect();
    let marker_results = parsed
        .marker_results
        .unwrap_or_default()
        .into_iter()
        .map(|m| MarkerResult {
            marker_id: m.marker_id,
            test_name: m.test_name,
            scope: m.scope,
            collected: m.collected,
            investigation: m.investigation,
        })
        .collect();
    let output = RedOutput {
        summary,
        tests,
        skeletons,
        markers,
        marker_results,
        test_command: parsed.test_command.filter(|x| !x.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|x| !x.is_empty()),
        test_output_file: parsed.test_output_file.filter(|x| !x.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|x| !x.is_empty()),
        sequential_command: parsed.sequential_command.filter(|x| !x.is_empty()),
        logging_command: parsed.logging_command.filter(|x| !x.is_empty()),
        metric_hooks: parsed.metric_hooks.filter(|x| !x.is_empty()),
        feedback_options: parsed.feedback_options.filter(|x| !x.is_empty()),
    };
    log::debug!(
        target: "tddy_workflow_recipes::parser",
        "parse_red_response: deserialized ({} markers); validating marker source_file paths",
        output.markers.len()
    );
    validate_red_marker_source_paths(&output)?;
    log::debug!(
        target: "tddy_workflow_recipes::parser",
        "parse_red_response: marker placement validation ok"
    );
    Ok(output)
}

/// Validate that red output markers with `source_file` are only associated with production paths.
///
/// Callers invoke this after [`parse_red_response`] when enforcing production-only marker placement.
/// [`parse_red_response`] already runs this check; calling again is idempotent.
pub fn validate_red_marker_source_paths(output: &RedOutput) -> Result<(), ParseError> {
    log::debug!(
        target: "tddy_workflow_recipes::parser",
        "validate_red_marker_source_paths: checking {} markers for source_file paths",
        output.markers.len()
    );
    for m in &output.markers {
        let Some(ref path) = m.source_file else {
            log::debug!(
                target: "tddy_workflow_recipes::parser",
                "validate_red_marker_source_paths: marker {} has no source_file; skipping placement check",
                m.marker_id
            );
            continue;
        };
        if classify_rust_source_path(path) == RustSourcePathKind::Test {
            let msg = format!(
                "red marker {}: source_file {:?} is test-only; logging markers MUST NOT appear in test code — place markers only on production/skeleton entry points",
                m.marker_id, path
            );
            log::debug!(
                target: "tddy_workflow_recipes::parser",
                "validate_red_marker_source_paths: rejected marker_id={} test-only source_file={:?}",
                m.marker_id,
                path
            );
            return Err(ParseError::Malformed(msg));
        }
    }
    Ok(())
}

impl RedOutput {
    /// Render red goal output as markdown for red-output.md artifact.
    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Red Phase Output\n\n");
        out.push_str("## Summary\n\n");
        out.push_str(&self.summary);
        out.push_str("\n\n## How to run tests\n\n");
        out.push_str(
            self.test_command
                .as_deref()
                .unwrap_or("(Inspect the project to determine the test command, e.g. `cargo test`, `npm test`, `pytest`)"),
        );
        out.push_str("\n\n## Prerequisite actions\n\n");
        out.push_str(
            self.prerequisite_actions
                .as_deref()
                .unwrap_or("None. Use the cheapest approach: if the test command already builds or bundles, do not run a separate build."),
        );
        out.push_str("\n\n## How to run a single or selected tests\n\n");
        out.push_str(
            self.run_single_or_selected_tests
                .as_deref()
                .unwrap_or("(Inspect the project: e.g. `cargo test <name>`, `pytest -k <pattern>`, `npm test -- --testNamePattern=<pattern>`)"),
        );
        out.push_str("\n\n## Tests\n\n");
        for t in &self.tests {
            out.push_str(&format!("### {}\n", t.name));
            out.push_str(&format!("- **File**: {}\n", t.file));
            out.push_str(&format!("- **Line**: {}\n", t.line.unwrap_or(0)));
            out.push_str(&format!("- **Status**: {}\n\n", t.status));
        }
        out.push_str("## Skeletons\n\n");
        for s in &self.skeletons {
            out.push_str(&format!("### {}\n", s.name));
            out.push_str(&format!("- **File**: {}\n", s.file));
            out.push_str(&format!("- **Line**: {}\n", s.line.unwrap_or(0)));
            out.push_str(&format!("- **Kind**: {}\n\n", s.kind));
        }
        if !self.markers.is_empty() {
            out.push_str("## Logging Markers\n\n");
            for m in &self.markers {
                out.push_str(&format!(
                    "- **{}** (scope: {}): {}\n",
                    m.marker_id, m.scope, m.test_name
                ));
            }
        }
        if !self.marker_results.is_empty() {
            out.push_str("\n## Marker Verification\n\n");
            for r in &self.marker_results {
                out.push_str(&format!(
                    "- **{}**: collected={}\n",
                    r.marker_id, r.collected
                ));
            }
        }
        out
    }

    /// Render progress.md with unfilled checkboxes for failed tests and skeletons.
    /// Next goal uses this to mark items as done, skipped, or failed.
    pub fn to_progress_markdown(&self) -> String {
        let mut out = String::from("# Progress\n\n");
        out.push_str("Unfilled milestones. Mark each as done [x], skipped, or failed.\n\n");
        out.push_str("## Failed Tests\n\n");
        for t in &self.tests {
            let loc = t
                .line
                .map(|l| format!("{}:{}", t.file, l))
                .unwrap_or_else(|| t.file.clone());
            out.push_str(&format!("- [ ] {} ({})\n", t.name, loc));
        }
        out.push_str("\n## Skeletons\n\n");
        for s in &self.skeletons {
            let loc = s
                .line
                .map(|l| format!("{}:{}", s.file, l))
                .unwrap_or_else(|| s.file.clone());
            out.push_str(&format!("- [ ] {} ({}) — {}\n", s.name, loc, s.kind));
        }
        out
    }
}
