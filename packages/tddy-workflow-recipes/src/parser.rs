//! Parser for LLM structured output.
//!
//! All structured output is received via `tddy-tools submit` (Unix socket IPC).
//! Parser functions accept pre-validated JSON strings and deserialize into typed structs.
//! Questions are extracted from AskUserQuestion tool events in the NDJSON stream, not from text.

use tddy_core::error::ParseError;
use tddy_core::source_path::{classify_rust_source_path, RustSourcePathKind};

/// Parsed planning output. PRD must include a `## TODO` section (implementation milestones).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanningOutput {
    pub prd: String,
    /// PRD/feature name from plan agent (e.g. "Auth Feature").
    pub name: Option<String>,
    /// Discovery data (toolchain, scripts, doc locations) from plan goal.
    pub discovery: Option<tddy_core::changeset::DiscoveryData>,
    /// Demo plan for user verification.
    pub demo_plan: Option<DemoPlan>,
    /// Daemon mode: suggested git branch name for the feature.
    pub branch_suggestion: Option<String>,
    /// Daemon mode: suggested worktree directory name (e.g. "feature-auth").
    pub worktree_suggestion: Option<String>,
}

/// Demo plan for presenting the feature to the user.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoPlan {
    pub demo_type: String,
    pub setup_instructions: String,
    pub steps: Vec<DemoStep>,
    pub verification: String,
}

/// A single demo step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoStep {
    pub description: String,
    pub command_or_action: String,
    pub expected_result: String,
}

#[derive(serde::Deserialize)]
struct StructuredPlan {
    goal: Option<String>,
    name: Option<String>,
    prd: Option<String>,
    discovery: Option<tddy_core::changeset::DiscoveryData>,
    demo_plan: Option<DemoPlan>,
    branch_suggestion: Option<String>,
    worktree_suggestion: Option<String>,
}

/// Parse LLM planning response. JSON must come from tddy-tools submit (no inline parsing).
pub fn parse_planning_response(s: &str) -> Result<PlanningOutput, ParseError> {
    parse_planning_response_impl(s, None)
}

/// Like parse_planning_response but resolves `prd` when it is a path to an MD file (relative to base_path).
pub fn parse_planning_response_with_base(
    s: &str,
    _base_path: &std::path::Path,
) -> Result<PlanningOutput, ParseError> {
    parse_planning_response_impl(s, Some(_base_path))
}

/// Heuristic: `prd` is a relative markdown file reference, not inline PRD body (which has newlines).
fn prd_value_looks_like_md_file_path(prd: &str) -> bool {
    const MAX_PRD_FILE_PATH_REF_LEN: usize = 260;
    let t = prd.trim();
    t.len() <= MAX_PRD_FILE_PATH_REF_LEN
        && !t.contains('\n')
        && !t.contains('\r')
        && t.ends_with(".md")
}

fn parse_planning_response_impl(
    s: &str,
    _base_path: Option<&std::path::Path>,
) -> Result<PlanningOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredPlan = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("plan") {
        return Err(ParseError::Malformed("goal is not plan".into()));
    }
    let mut prd = parsed
        .prd
        .filter(|x| !x.trim().is_empty())
        .ok_or_else(|| ParseError::Malformed("prd missing or empty".into()))?;
    if let Some(base) = _base_path {
        let path = base.join(prd.trim());
        if path.exists() && path.is_file() {
            prd = std::fs::read_to_string(&path).map_err(|e| {
                ParseError::Malformed(format!("failed to read prd file {}: {}", path.display(), e))
            })?;
        } else if prd_value_looks_like_md_file_path(&prd) {
            return Err(ParseError::Malformed(format!(
                "prd references markdown file {:?} but no such file was found under {}",
                prd.trim(),
                base.display()
            )));
        }
    }
    Ok(PlanningOutput {
        prd,
        name: parsed.name.filter(|s| !s.is_empty()),
        discovery: parsed.discovery,
        demo_plan: parsed.demo_plan,
        branch_suggestion: parsed.branch_suggestion.filter(|s| !s.is_empty()),
        worktree_suggestion: parsed.worktree_suggestion.filter(|s| !s.is_empty()),
    })
}

/// Parsed acceptance tests output.
#[derive(Debug, Clone)]
pub struct AcceptanceTestsOutput {
    pub summary: String,
    pub tests: Vec<AcceptanceTestInfo>,
    /// How to run the tests, derived from project (e.g. "cargo test", "npm test").
    pub test_command: Option<String>,
    /// Prerequisite actions before running tests (e.g. "None" or "Run cargo build first"). Use cheapest way: omit if test script already builds.
    pub prerequisite_actions: Option<String>,
    /// How to run a single or selected tests (e.g. "cargo test <name>", "pytest -k <pattern>").
    pub run_single_or_selected_tests: Option<String>,
    /// How to run tests sequentially (e.g. "cargo test -- --test-threads=1").
    pub sequential_command: Option<String>,
    /// How to run tests with logging (e.g. "RUST_LOG=debug cargo test").
    pub logging_command: Option<String>,
    /// Metric reporting hooks (e.g. "cargo test -- --format json").
    pub metric_hooks: Option<String>,
    /// Execution feedback options (e.g. "cargo test 2>&1 | tee test-output.txt").
    pub feedback_options: Option<String>,
}

/// Info about a single acceptance test.
#[derive(Debug, Clone)]
pub struct AcceptanceTestInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub status: String,
}

impl AcceptanceTestsOutput {
    /// Render acceptance tests output as markdown for acceptance-tests.md artifact.
    pub fn to_markdown(&self) -> String {
        let mut out = String::from("# Acceptance Tests\n\n");
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
            out.push_str(&format!("- **Status**: {}\n", t.status));
            out.push_str(&format!(
                "- **Validates**: {}\n\n",
                t.name.replace('_', " ")
            ));
        }
        out
    }
}

#[derive(serde::Deserialize)]
struct StructuredAcceptanceTests {
    goal: Option<String>,
    summary: Option<String>,
    tests: Option<Vec<AcceptanceTestInfoDe>>,
    test_command: Option<String>,
    prerequisite_actions: Option<String>,
    run_single_or_selected_tests: Option<String>,
    #[serde(default)]
    sequential_command: Option<String>,
    #[serde(default)]
    logging_command: Option<String>,
    #[serde(default)]
    metric_hooks: Option<String>,
    #[serde(default)]
    feedback_options: Option<String>,
}

#[derive(serde::Deserialize)]
struct AcceptanceTestInfoDe {
    name: String,
    file: String,
    line: Option<u32>,
    status: String,
}

/// Parse LLM acceptance tests response. JSON must come from tddy-tools submit.
pub fn parse_acceptance_tests_response(s: &str) -> Result<AcceptanceTestsOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredAcceptanceTests = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("acceptance-tests") {
        return Err(ParseError::Malformed("goal is not acceptance-tests".into()));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    let tests = parsed
        .tests
        .unwrap_or_default()
        .into_iter()
        .map(|t| AcceptanceTestInfo {
            name: t.name,
            file: t.file,
            line: t.line,
            status: t.status,
        })
        .collect();
    Ok(AcceptanceTestsOutput {
        summary,
        tests,
        test_command: parsed.test_command.filter(|x| !x.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|x| !x.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|x| !x.is_empty()),
        sequential_command: parsed.sequential_command.filter(|x| !x.is_empty()),
        logging_command: parsed.logging_command.filter(|x| !x.is_empty()),
        metric_hooks: parsed.metric_hooks.filter(|x| !x.is_empty()),
        feedback_options: parsed.feedback_options.filter(|x| !x.is_empty()),
    })
}

// ── analyze output (bugfix pipeline) ─────────────────────────────────────────

/// Parsed output from the bugfix `analyze` goal (`tddy-tools submit --goal analyze`).
#[derive(Debug, Clone)]
pub struct AnalyzeOutput {
    pub branch_suggestion: String,
    pub worktree_suggestion: String,
    pub name: Option<String>,
    pub summary: Option<String>,
}

#[derive(serde::Deserialize)]
struct StructuredAnalyze {
    goal: Option<String>,
    branch_suggestion: Option<String>,
    worktree_suggestion: Option<String>,
    name: Option<String>,
    summary: Option<String>,
}

/// Parse LLM analyze response. JSON must come from tddy-tools submit.
pub fn parse_analyze_response(s: &str) -> Result<AnalyzeOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredAnalyze = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("analyze") {
        return Err(ParseError::Malformed(format!(
            "goal is not analyze, got: {:?}",
            parsed.goal
        )));
    }
    let branch_suggestion = parsed
        .branch_suggestion
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("branch_suggestion missing or empty".into()))?;
    let worktree_suggestion = parsed
        .worktree_suggestion
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("worktree_suggestion missing or empty".into()))?;
    Ok(AnalyzeOutput {
        branch_suggestion,
        worktree_suggestion,
        name: parsed.name.filter(|x| !x.is_empty()),
        summary: parsed.summary.filter(|x| !x.is_empty()),
    })
}

/// Parsed green goal output.
#[derive(Debug, Clone)]
pub struct GreenOutput {
    pub summary: String,
    pub tests: Vec<GreenTestResult>,
    pub implementations: Vec<ImplementationInfo>,
    pub test_command: Option<String>,
    pub prerequisite_actions: Option<String>,
    pub run_single_or_selected_tests: Option<String>,
    /// Demo results when demo-plan.md was present and green completed.
    pub demo_results: Option<DemoResults>,
}

/// Demo execution results from green goal.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemoResults {
    pub summary: String,
    pub steps_completed: u32,
}

/// Parsed output from the standalone demo goal.
#[derive(Debug, Clone)]
pub struct DemoOutput {
    pub summary: String,
    pub demo_type: String,
    pub steps_completed: u32,
    pub verification: String,
}

/// Info about a single test result from the green goal.
#[derive(Debug, Clone)]
pub struct GreenTestResult {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub status: String,
    pub reason: Option<String>,
}

/// Info about an implementation (method, struct, etc.) from the green goal.
#[derive(Debug, Clone)]
pub struct ImplementationInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
}

#[derive(serde::Deserialize)]
struct StructuredGreen {
    goal: Option<String>,
    summary: Option<String>,
    tests: Option<Vec<GreenTestResultDe>>,
    implementations: Option<Vec<ImplementationInfoDe>>,
    test_command: Option<String>,
    prerequisite_actions: Option<String>,
    run_single_or_selected_tests: Option<String>,
    #[serde(default)]
    demo_results: Option<DemoResultsDe>,
}

#[derive(serde::Deserialize)]
struct DemoResultsDe {
    summary: String,
    steps_completed: u32,
}

#[derive(serde::Deserialize)]
struct GreenTestResultDe {
    name: String,
    file: String,
    line: Option<u32>,
    status: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct ImplementationInfoDe {
    name: String,
    file: String,
    line: Option<u32>,
    kind: String,
}

/// Parse LLM green goal response. JSON must come from tddy-tools submit.
pub fn parse_green_response(s: &str) -> Result<GreenOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredGreen = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("green") {
        return Err(ParseError::Malformed("goal is not green".into()));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    let tests = parsed
        .tests
        .unwrap_or_default()
        .into_iter()
        .map(|t| GreenTestResult {
            name: t.name,
            file: t.file,
            line: t.line,
            status: t.status,
            reason: t.reason,
        })
        .collect();
    let implementations = parsed
        .implementations
        .unwrap_or_default()
        .into_iter()
        .map(|i| ImplementationInfo {
            name: i.name,
            file: i.file,
            line: i.line,
            kind: i.kind,
        })
        .collect();
    let demo_results = parsed.demo_results.map(|d| DemoResults {
        summary: d.summary,
        steps_completed: d.steps_completed,
    });
    Ok(GreenOutput {
        summary,
        tests,
        implementations,
        test_command: parsed.test_command.filter(|x| !x.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|x| !x.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|x| !x.is_empty()),
        demo_results,
    })
}

impl GreenOutput {
    /// Render updated progress.md with [x] for passing, [!] for failing.
    pub fn to_updated_progress_markdown(&self) -> String {
        let mut out = String::from("# Progress\n\n");
        out.push_str("Unfilled milestones. Mark each as done [x], skipped, or failed.\n\n");
        out.push_str("## Failed Tests\n\n");
        for t in &self.tests {
            let loc = t
                .line
                .map(|l| format!("{}:{}", t.file, l))
                .unwrap_or_else(|| t.file.clone());
            let marker = if t.status == "passing" { "[x]" } else { "[!]" };
            let reason = t
                .reason
                .as_deref()
                .map(|r| format!(" — {}", r))
                .unwrap_or_default();
            out.push_str(&format!("- {} {} ({}){}\n", marker, t.name, loc, reason));
        }
        out.push_str("\n## Skeletons\n\n");
        for i in &self.implementations {
            let loc = i
                .line
                .map(|l| format!("{}:{}", i.file, l))
                .unwrap_or_else(|| i.file.clone());
            out.push_str(&format!("- [x] {} ({}) — {}\n", i.name, loc, i.kind));
        }
        out
    }

    /// Update acceptance-tests.md content: replace "failing" with "passing" for passing tests.
    pub fn update_acceptance_tests_content(&self, content: &str) -> String {
        let passing: std::collections::HashSet<&str> = self
            .tests
            .iter()
            .filter(|t| t.status == "passing")
            .map(|t| t.name.as_str())
            .collect();
        if passing.is_empty() {
            return content.to_string();
        }
        let mut out = String::new();
        let sections: Vec<&str> = content.split("\n### ").collect();
        for (i, section) in sections.iter().enumerate() {
            if i == 0 {
                out.push_str(section);
                if sections.len() > 1 {
                    out.push_str("\n### ");
                }
                continue;
            }
            let (name, rest) = section.split_once('\n').unwrap_or((section, ""));
            let test_name = name.trim();
            let updated_rest = if passing.contains(test_name) {
                rest.replace("- **Status**: failing", "- **Status**: passing")
            } else {
                rest.to_string()
            };
            out.push_str(test_name);
            out.push('\n');
            out.push_str(&updated_rest);
            if i < sections.len() - 1 {
                out.push_str("\n### ");
            }
        }
        out
    }

    /// Returns true if all tests are passing.
    pub fn all_tests_passing(&self) -> bool {
        self.tests.iter().all(|t| t.status == "passing")
    }
}

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
    goal: Option<String>,
    summary: Option<String>,
    tests: Option<Vec<RedTestInfoDe>>,
    skeletons: Option<Vec<SkeletonInfoDe>>,
    test_command: Option<String>,
    prerequisite_actions: Option<String>,
    run_single_or_selected_tests: Option<String>,
    #[serde(default)]
    markers: Option<Vec<MarkerInfoDe>>,
    #[serde(default)]
    marker_results: Option<Vec<MarkerResultDe>>,
    #[serde(default)]
    test_output_file: Option<String>,
    #[serde(default)]
    sequential_command: Option<String>,
    #[serde(default)]
    logging_command: Option<String>,
    #[serde(default)]
    metric_hooks: Option<String>,
    #[serde(default)]
    feedback_options: Option<String>,
}

#[derive(serde::Deserialize)]
struct MarkerInfoDe {
    marker_id: String,
    test_name: String,
    scope: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    source_file: Option<String>,
}

#[derive(serde::Deserialize)]
struct MarkerResultDe {
    marker_id: String,
    test_name: String,
    scope: String,
    collected: bool,
    investigation: Option<String>,
}

#[derive(serde::Deserialize)]
struct RedTestInfoDe {
    name: String,
    file: String,
    line: Option<u32>,
    status: String,
}

#[derive(serde::Deserialize)]
struct SkeletonInfoDe {
    name: String,
    file: String,
    line: Option<u32>,
    kind: String,
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

/// Build result entry from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateBuildResult {
    pub package: String,
    pub status: String,
    pub notes: Option<String>,
}

/// An issue found during evaluation.
#[derive(Debug, Clone)]
pub struct EvaluateIssue {
    pub severity: String,
    pub category: String,
    pub file: String,
    pub line: Option<u32>,
    pub description: String,
    pub suggestion: Option<String>,
}

/// Changeset sync status from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateChangesetSync {
    pub status: String,
    pub items_updated: u32,
    pub items_added: u32,
}

/// File analyzed entry from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateFileAnalyzed {
    pub file: String,
    pub lines_changed: Option<u32>,
    pub changeset_item: Option<String>,
}

/// Test impact summary from evaluate-changes output.
#[derive(Debug, Clone)]
pub struct EvaluateTestImpact {
    pub tests_affected: u32,
    pub new_tests_needed: u32,
}

#[derive(serde::Deserialize)]
struct EvaluateBuildResultDe {
    package: String,
    status: String,
    notes: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateIssueDe {
    severity: String,
    category: String,
    file: String,
    line: Option<u32>,
    description: String,
    suggestion: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateChangesetSyncDe {
    status: String,
    #[serde(default)]
    items_updated: u32,
    #[serde(default)]
    items_added: u32,
}

#[derive(serde::Deserialize)]
struct EvaluateFileAnalyzedDe {
    file: String,
    lines_changed: Option<u32>,
    changeset_item: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateTestImpactDe {
    tests_affected: u32,
    new_tests_needed: u32,
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

// ── evaluate-changes output types ────────────────────────────────────────────

/// A changed file entry in an evaluate-changes report.
#[derive(Debug, Clone)]
pub struct EvaluateChangedFile {
    pub path: String,
    pub change_type: String,
    pub lines_added: i64,
    pub lines_removed: i64,
}

/// An affected test entry in an evaluate-changes report.
#[derive(Debug, Clone)]
pub struct EvaluateAffectedTest {
    pub path: String,
    pub status: String,
    pub description: String,
}

/// Parsed output from the evaluate-changes goal.
#[derive(Debug, Clone)]
pub struct EvaluateOutput {
    pub summary: String,
    pub risk_level: String,
    pub build_results: Vec<EvaluateBuildResult>,
    pub issues: Vec<EvaluateIssue>,
    pub changeset_sync: Option<EvaluateChangesetSync>,
    pub files_analyzed: Vec<EvaluateFileAnalyzed>,
    pub test_impact: Option<EvaluateTestImpact>,
    pub changed_files: Vec<EvaluateChangedFile>,
    pub affected_tests: Vec<EvaluateAffectedTest>,
    pub validity_assessment: String,
}

#[derive(serde::Deserialize)]
struct StructuredEvaluate {
    goal: Option<String>,
    summary: Option<String>,
    risk_level: Option<String>,
    #[serde(default)]
    build_results: Option<Vec<EvaluateBuildResultDe>>,
    #[serde(default)]
    issues: Option<Vec<EvaluateIssueDe>>,
    #[serde(default)]
    changeset_sync: Option<EvaluateChangesetSyncDe>,
    #[serde(default)]
    files_analyzed: Option<Vec<EvaluateFileAnalyzedDe>>,
    #[serde(default)]
    test_impact: Option<EvaluateTestImpactDe>,
    #[serde(default)]
    changed_files: Option<Vec<EvaluateChangedFileDe>>,
    #[serde(default)]
    affected_tests: Option<Vec<EvaluateAffectedTestDe>>,
    #[serde(default)]
    validity_assessment: Option<String>,
}

#[derive(serde::Deserialize)]
struct EvaluateChangedFileDe {
    path: String,
    change_type: String,
    #[serde(default)]
    lines_added: i64,
    #[serde(default)]
    lines_removed: i64,
}

#[derive(serde::Deserialize)]
struct EvaluateAffectedTestDe {
    path: String,
    status: String,
    #[serde(default)]
    description: String,
}

/// Parse LLM evaluate-changes response. JSON must come from tddy-tools submit.
pub fn parse_evaluate_response(s: &str) -> Result<EvaluateOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredEvaluate = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("evaluate-changes") {
        return Err(ParseError::Malformed(format!(
            "goal is not evaluate-changes, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "No summary provided.".to_string());
    let risk_level = parsed
        .risk_level
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let build_results = parsed
        .build_results
        .unwrap_or_default()
        .into_iter()
        .map(|b| EvaluateBuildResult {
            package: b.package,
            status: b.status,
            notes: b.notes,
        })
        .collect();
    let issues = parsed
        .issues
        .unwrap_or_default()
        .into_iter()
        .map(|i| EvaluateIssue {
            severity: i.severity,
            category: i.category,
            file: i.file,
            line: i.line,
            description: i.description,
            suggestion: i.suggestion,
        })
        .collect();
    let changeset_sync = parsed.changeset_sync.map(|c| EvaluateChangesetSync {
        status: c.status,
        items_updated: c.items_updated,
        items_added: c.items_added,
    });
    let files_analyzed = parsed
        .files_analyzed
        .unwrap_or_default()
        .into_iter()
        .map(|f| EvaluateFileAnalyzed {
            file: f.file,
            lines_changed: f.lines_changed,
            changeset_item: f.changeset_item,
        })
        .collect();
    let test_impact = parsed.test_impact.map(|t| EvaluateTestImpact {
        tests_affected: t.tests_affected,
        new_tests_needed: t.new_tests_needed,
    });
    let changed_files: Vec<_> = parsed
        .changed_files
        .unwrap_or_default()
        .into_iter()
        .map(|c| EvaluateChangedFile {
            path: c.path,
            change_type: c.change_type,
            lines_added: c.lines_added,
            lines_removed: c.lines_removed,
        })
        .collect();
    let affected_tests: Vec<_> = parsed
        .affected_tests
        .unwrap_or_default()
        .into_iter()
        .map(|a| EvaluateAffectedTest {
            path: a.path,
            status: a.status,
            description: a.description,
        })
        .collect();
    let validity_assessment = parsed
        .validity_assessment
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    log::debug!(
        "[tddy-core] parse_evaluate_response: parsed {} changed_files, {} affected_tests",
        changed_files.len(),
        affected_tests.len()
    );

    Ok(EvaluateOutput {
        summary,
        risk_level,
        build_results,
        issues,
        changeset_sync,
        files_analyzed,
        test_impact,
        changed_files,
        affected_tests,
        validity_assessment,
    })
}

// ── validate (subagents) output types ─────────────────────────────────────────

/// Parsed output from the validate goal (subagent-based).
#[derive(Debug, Clone)]
pub struct ValidateSubagentsOutput {
    pub goal: String,
    pub summary: String,
    pub tests_report_written: bool,
    pub prod_ready_report_written: bool,
    pub clean_code_report_written: bool,
    pub refactoring_plan_written: bool,
    /// Markdown body for `refactoring-plan.md` when included in `tddy-tools submit` JSON.
    pub refactoring_plan: Option<String>,
}

#[derive(serde::Deserialize)]
struct StructuredValidateRefactor {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tests_report_written: Option<bool>,
    #[serde(default)]
    prod_ready_report_written: Option<bool>,
    #[serde(default)]
    clean_code_report_written: Option<bool>,
    #[serde(default)]
    refactoring_plan_written: Option<bool>,
    #[serde(default)]
    refactoring_plan: Option<String>,
}

/// Parse LLM validate (subagent) response. JSON must come from tddy-tools submit.
pub fn parse_validate_subagents_response(s: &str) -> Result<ValidateSubagentsOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredValidateRefactor = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("validate") {
        return Err(ParseError::Malformed(format!(
            "goal must be validate, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| "No summary provided.".to_string());

    log::debug!(
        "[tddy-core] parse_validate_subagents_response: summary length={}, tests_written={:?}",
        summary.len(),
        parsed.tests_report_written
    );

    Ok(ValidateSubagentsOutput {
        goal: "validate".to_string(),
        summary,
        tests_report_written: parsed.tests_report_written.unwrap_or(false),
        prod_ready_report_written: parsed.prod_ready_report_written.unwrap_or(false),
        clean_code_report_written: parsed.clean_code_report_written.unwrap_or(false),
        refactoring_plan_written: parsed.refactoring_plan_written.unwrap_or(false),
        refactoring_plan: parsed.refactoring_plan.filter(|s| !s.trim().is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_planning_response_accepts_valid_json() {
        let input = "{\"goal\":\"plan\",\"prd\":\"# PRD\\n\\n## Summary\\nFeature X\\n\\n## TODO\\n\\n- [ ] Task 1\"}";
        let out = parse_planning_response(input).expect("should parse");
        assert!(out.prd.contains("Feature X"));
        assert!(out.prd.contains("Task 1"));
    }

    #[test]
    fn parse_planning_response_rejects_non_json() {
        let input = "Some random text without JSON";
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_rejects_wrong_goal() {
        let input = "{\"goal\":\"red\",\"prd\":\"# PRD\\n\\n## TODO\\n\\n- [ ] T1\"}";
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_rejects_empty_prd() {
        let input = r#"{"goal":"plan","prd":"   "}"#;
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn red_output_to_progress_markdown_produces_unfilled_checkboxes() {
        use super::{RedOutput, RedTestInfo, SkeletonInfo};
        let out = RedOutput {
            summary: "Created skeletons.".into(),
            tests: vec![
                RedTestInfo {
                    name: "test_foo".into(),
                    file: "src/foo.rs".into(),
                    line: Some(10),
                    status: "failing".into(),
                },
                RedTestInfo {
                    name: "test_bar".into(),
                    file: "src/bar.rs".into(),
                    line: None,
                    status: "failing".into(),
                },
            ],
            skeletons: vec![SkeletonInfo {
                name: "Foo".into(),
                file: "src/foo.rs".into(),
                line: Some(5),
                kind: "struct".into(),
            }],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            markers: vec![],
            marker_results: vec![],
            test_output_file: None,
            sequential_command: None,
            logging_command: None,
            metric_hooks: None,
            feedback_options: None,
        };
        let md = out.to_progress_markdown();
        assert!(md.contains("## Failed Tests"));
        assert!(md.contains("## Skeletons"));
        assert!(md.contains("- [ ] test_foo (src/foo.rs:10)"));
        assert!(md.contains("- [ ] test_bar (src/bar.rs)"));
        assert!(md.contains("- [ ] Foo (src/foo.rs:5) — struct"));
    }

    #[test]
    fn parse_red_response_extracts_summary_tests_skeletons() {
        let input = r#"{"goal":"red","summary":"Created 2 skeletons and 1 failing test.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":5,"kind":"struct"},{"name":"bar","file":"src/foo.rs","line":8,"kind":"method"}]}"#;
        let out = super::parse_red_response(input).expect("should parse");
        assert!(out.summary.contains("2 skeletons"));
        assert_eq!(out.tests.len(), 1);
        assert_eq!(out.tests[0].name, "test_foo");
        assert_eq!(out.tests[0].file, "src/foo.rs");
        assert_eq!(out.tests[0].line, Some(10));
        assert_eq!(out.tests[0].status, "failing");
        assert_eq!(out.skeletons.len(), 2);
        assert_eq!(out.skeletons[0].name, "Foo");
        assert_eq!(out.skeletons[0].kind, "struct");
        assert_eq!(out.skeletons[1].name, "bar");
        assert_eq!(out.skeletons[1].kind, "method");
    }

    #[test]
    fn parse_red_response_extracts_test_command_and_prerequisite_actions() {
        let input = r#"{"goal":"red","summary":"Created skeletons.","tests":[],"skeletons":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;
        let out = super::parse_red_response(input).expect("should parse");
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn validate_red_marker_source_paths_accepts_production_only_markers() {
        let out = RedOutput {
            summary: "s".into(),
            tests: vec![],
            skeletons: vec![],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            markers: vec![MarkerInfo {
                marker_id: "M001".into(),
                test_name: "t".into(),
                scope: "scope".into(),
                data: serde_json::json!({}),
                source_file: Some("packages/demo/src/widget.rs".into()),
            }],
            marker_results: vec![],
            test_output_file: None,
            sequential_command: None,
            logging_command: None,
            metric_hooks: None,
            feedback_options: None,
        };
        validate_red_marker_source_paths(&out).expect("production-only markers should validate");
    }

    #[test]
    fn parse_acceptance_tests_response_extracts_summary_and_tests() {
        use super::parse_acceptance_tests_response;
        let input = r#"{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}"#;
        let out = parse_acceptance_tests_response(input).expect("should parse");
        assert!(out.summary.contains("Created 2 acceptance tests"));
        assert_eq!(out.tests.len(), 2);
        assert_eq!(out.tests[0].name, "login_stores_session_token");
        assert_eq!(out.tests[0].file, "packages/auth/tests/session.it.rs");
        assert_eq!(out.tests[0].line, Some(15));
        assert_eq!(out.tests[0].status, "failing");
    }

    #[test]
    fn parse_acceptance_tests_response_extracts_test_command_and_prerequisite_actions() {
        let input = r#"{"goal":"acceptance-tests","summary":"Created 2 tests.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;
        let out = super::parse_acceptance_tests_response(input).expect("should parse");
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn parse_green_response_extracts_summary_tests_implementations() {
        let input = r#"{"goal":"green","summary":"Implemented 2 methods. All tests passing.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"passing"},{"name":"test_bar","file":"src/bar.rs","line":20,"status":"failing","reason":"timeout"}],"implementations":[{"name":"AuthService::validate","file":"src/service.rs","line":15,"kind":"method"}]}"#;
        let out = parse_green_response(input).expect("should parse");
        assert!(out.summary.contains("All tests passing"));
        assert_eq!(out.tests.len(), 2);
        assert_eq!(out.tests[0].name, "test_foo");
        assert_eq!(out.tests[0].status, "passing");
        assert_eq!(out.tests[1].status, "failing");
        assert_eq!(out.tests[1].reason.as_deref(), Some("timeout"));
        assert_eq!(out.implementations.len(), 1);
        assert_eq!(out.implementations[0].name, "AuthService::validate");
        assert_eq!(out.implementations[0].kind, "method");
    }

    #[test]
    fn parse_green_response_extracts_test_command_fields() {
        let input = r#"{"goal":"green","summary":"Implemented.","tests":[],"implementations":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;
        let out = parse_green_response(input).expect("should parse");
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn parse_green_response_errors_on_wrong_goal() {
        let input = r#"{"goal":"red","summary":"Wrong goal.","tests":[],"implementations":[]}"#;
        let err = parse_green_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn green_output_to_updated_progress_markdown_marks_passing_and_failing() {
        use super::{GreenOutput, GreenTestResult, ImplementationInfo};
        let out = GreenOutput {
            summary: "Implemented.".into(),
            tests: vec![
                GreenTestResult {
                    name: "test_foo".into(),
                    file: "src/foo.rs".into(),
                    line: Some(10),
                    status: "passing".into(),
                    reason: None,
                },
                GreenTestResult {
                    name: "test_bar".into(),
                    file: "src/bar.rs".into(),
                    line: Some(20),
                    status: "failing".into(),
                    reason: Some("timeout".into()),
                },
            ],
            implementations: vec![ImplementationInfo {
                name: "Foo".into(),
                file: "src/foo.rs".into(),
                line: Some(5),
                kind: "struct".into(),
            }],
            test_command: None,
            prerequisite_actions: None,
            run_single_or_selected_tests: None,
            demo_results: None,
        };
        let md = out.to_updated_progress_markdown();
        assert!(md.contains("- [x] test_foo"));
        assert!(md.contains("- [!] test_bar"));
        assert!(md.contains("timeout"));
        assert!(md.contains("- [x] Foo"));
    }
}

/// Parse the standalone demo goal. JSON must come from tddy-tools submit.
pub fn parse_demo_response(s: &str) -> Result<DemoOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredDemo = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("demo") {
        return Err(ParseError::Malformed(format!(
            "goal is not demo, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;

    log::debug!(
        "[tddy-core] parse_demo_response: summary_len={}, steps={}",
        summary.len(),
        parsed.steps_completed.unwrap_or(0)
    );

    Ok(DemoOutput {
        summary,
        demo_type: parsed.demo_type.unwrap_or_else(|| "unknown".to_string()),
        steps_completed: parsed.steps_completed.unwrap_or(0),
        verification: parsed.verification.unwrap_or_default(),
    })
}

// ── refactor output types ────────────────────────────────────────────────────

/// Parsed output from the refactor goal.
#[derive(Debug, Clone)]
pub struct RefactorOutput {
    pub summary: String,
    pub tasks_completed: u32,
    pub tests_passing: bool,
}

#[derive(serde::Deserialize)]
struct StructuredRefactor {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    tasks_completed: Option<u32>,
    #[serde(default)]
    tests_passing: Option<bool>,
}

/// Parse LLM refactor response. JSON must come from tddy-tools submit.
pub fn parse_refactor_response(s: &str) -> Result<RefactorOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredRefactor = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("refactor") {
        return Err(ParseError::Malformed(format!(
            "goal is not refactor, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    Ok(RefactorOutput {
        summary,
        tasks_completed: parsed.tasks_completed.unwrap_or(0),
        tests_passing: parsed.tests_passing.unwrap_or(false),
    })
}

// ── update-docs output types ─────────────────────────────────────────────────

/// Parsed output from the update-docs goal.
#[derive(Debug, Clone)]
pub struct UpdateDocsOutput {
    pub summary: String,
    pub docs_updated: u32,
}

#[derive(serde::Deserialize)]
struct StructuredUpdateDocs {
    goal: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    docs_updated: Option<u32>,
}

/// Parse LLM update-docs response. JSON must come from tddy-tools submit.
pub fn parse_update_docs_response(s: &str) -> Result<UpdateDocsOutput, ParseError> {
    let s = s.trim();
    let parsed: StructuredUpdateDocs = serde_json::from_str(s)
        .map_err(|e| ParseError::Malformed(format!("invalid JSON: {}", e)))?;
    if parsed.goal.as_deref() != Some("update-docs") {
        return Err(ParseError::Malformed(format!(
            "goal is not update-docs, got: {:?}",
            parsed.goal
        )));
    }
    let summary = parsed
        .summary
        .filter(|x| !x.is_empty())
        .ok_or_else(|| ParseError::Malformed("summary missing or empty".into()))?;
    Ok(UpdateDocsOutput {
        summary,
        docs_updated: parsed.docs_updated.unwrap_or(0),
    })
}

#[derive(serde::Deserialize)]
struct StructuredDemo {
    goal: Option<String>,
    summary: Option<String>,
    demo_type: Option<String>,
    steps_completed: Option<u32>,
    verification: Option<String>,
}
