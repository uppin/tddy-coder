//! Parser for LLM planning output.
//!
//! Supports two formats (in order of precedence):
//! 1. Structured response: `<structured-response content-type="application-json">{"goal":"plan","prd":"...","todo":"..."}</structured-response>`
//! 2. Delimited: `---PRD_START---` / `---PRD_END---` and `---TODO_START---` / `---TODO_END---`
//!
//! Questions are extracted from AskUserQuestion tool events in the NDJSON stream, not from text.

use crate::error::ParseError;

const STRUCTURED_OPEN: &str = "<structured-response";
const STRUCTURED_CLOSE: &str = "</structured-response>";
const PRD_START: &str = "---PRD_START---";
const PRD_END: &str = "---PRD_END---";
const TODO_START: &str = "---TODO_START---";
const TODO_END: &str = "---TODO_END---";

/// Parsed planning output containing PRD and TODO content.
#[derive(Debug, Clone)]
pub struct PlanningOutput {
    pub prd: String,
    pub todo: String,
    /// PRD/feature name from plan agent (e.g. "Auth Feature").
    pub name: Option<String>,
    /// Discovery data (toolchain, scripts, doc locations) from plan goal.
    pub discovery: Option<crate::changeset::DiscoveryData>,
    /// Demo plan for user verification.
    pub demo_plan: Option<DemoPlan>,
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
    todo: Option<String>,
    discovery: Option<crate::changeset::DiscoveryData>,
    demo_plan: Option<DemoPlan>,
}

/// Extract JSON from <structured-response content-type="application-json">...</structured-response>.
/// Tries each block in order; the first that parses successfully is returned.
/// This handles output that contains the system prompt (with example block) before the model's response.
fn extract_structured_response(s: &str) -> Option<PlanningOutput> {
    let mut search_from = 0;
    while let Some(open) = s[search_from..].find(STRUCTURED_OPEN) {
        let open = search_from + open;
        let after_open = &s[open + STRUCTURED_OPEN.len()..];
        let Some(gt) = after_open.find('>') else {
            search_from = open + 1;
            continue;
        };
        let content = after_open[gt + 1..].trim();
        let Some(close) = content.find(STRUCTURED_CLOSE) else {
            search_from = open + 1;
            continue;
        };
        let json_str = content[..close].trim();
        if let Ok(parsed) = serde_json::from_str::<StructuredPlan>(json_str) {
            if parsed.goal.as_deref() == Some("plan") {
                if let (Some(prd), Some(todo)) = (
                    parsed.prd.filter(|s| !s.is_empty()),
                    parsed.todo.filter(|s| !s.is_empty()),
                ) {
                    return Some(PlanningOutput {
                        prd,
                        todo,
                        name: parsed.name.filter(|s| !s.is_empty()),
                        discovery: parsed.discovery,
                        demo_plan: parsed.demo_plan,
                    });
                }
            }
        }
        search_from = open + 1;
    }
    None
}

/// Parse LLM planning response: tries structured-response first, then delimited output.
/// Returns Malformed if neither format is found.
pub fn parse_planning_response(s: &str) -> Result<PlanningOutput, ParseError> {
    if let Some(out) = extract_structured_response(s) {
        return Ok(out);
    }
    if s.contains(PRD_START) && s.contains(TODO_START) {
        return parse_planning_output(s);
    }
    Err(ParseError::Malformed(
        "PRD/TODO delimiters not found. The agent must output either (1) a <structured-response content-type=\"application-json\"> block with {\"goal\":\"plan\",\"prd\":\"...\",\"todo\":\"...\"} or (2) ---PRD_START---/---PRD_END--- and ---TODO_START---/---TODO_END---. Meta-commentary or summaries without the actual plan content cause this error.".into(),
    ))
}

/// Parse LLM output that contains delimited PRD and TODO sections.
pub fn parse_planning_output(s: &str) -> Result<PlanningOutput, ParseError> {
    let prd = extract_section(s, PRD_START, PRD_END)
        .ok_or(ParseError::MissingPrd)?
        .trim()
        .to_string();

    let todo = extract_section(s, TODO_START, TODO_END)
        .ok_or(ParseError::MissingTodo)?
        .trim()
        .to_string();

    Ok(PlanningOutput {
        prd,
        todo,
        name: None,
        discovery: None,
        demo_plan: None,
    })
}

fn extract_section<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_idx = s.find(start)?;
    let content_start = start_idx + start.len();
    let rest = &s[content_start..];
    let end_idx = rest.find(end)?;
    Some(rest[..end_idx].trim())
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

/// Parse LLM acceptance tests response from structured-response block.
/// Returns Malformed if the expected format is not found.
pub fn parse_acceptance_tests_response(s: &str) -> Result<AcceptanceTestsOutput, ParseError> {
    let open = s
        .find(STRUCTURED_OPEN)
        .ok_or_else(|| ParseError::Malformed("structured-response not found".into()))?;
    let after_open = &s[open + STRUCTURED_OPEN.len()..];
    let gt = after_open
        .find('>')
        .ok_or_else(|| ParseError::Malformed("structured-response malformed".into()))?;
    let content = after_open[gt + 1..].trim();
    let close = content
        .find(STRUCTURED_CLOSE)
        .ok_or_else(|| ParseError::Malformed("structured-response close not found".into()))?;
    let json_str = content[..close].trim();
    if json_str.is_empty() {
        return Err(ParseError::Malformed(
            "structured-response block is empty — agent must output valid JSON between the tags"
                .into(),
        ));
    }
    let parsed: StructuredAcceptanceTests =
        serde_json::from_str(json_str).map_err(|e| ParseError::Malformed(e.to_string()))?;

    if parsed.goal.as_deref() != Some("acceptance-tests") {
        return Err(ParseError::Malformed("goal is not acceptance-tests".into()));
    }

    let summary = parsed
        .summary
        .filter(|s| !s.is_empty())
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
        test_command: parsed.test_command.filter(|s| !s.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|s| !s.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|s| !s.is_empty()),
        sequential_command: parsed.sequential_command.filter(|s| !s.is_empty()),
        logging_command: parsed.logging_command.filter(|s| !s.is_empty()),
        metric_hooks: parsed.metric_hooks.filter(|s| !s.is_empty()),
        feedback_options: parsed.feedback_options.filter(|s| !s.is_empty()),
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

/// Parse LLM green goal response from structured-response block.
pub fn parse_green_response(s: &str) -> Result<GreenOutput, ParseError> {
    let open = s
        .find(STRUCTURED_OPEN)
        .ok_or_else(|| ParseError::Malformed("structured-response not found".into()))?;
    let after_open = &s[open + STRUCTURED_OPEN.len()..];
    let gt = after_open
        .find('>')
        .ok_or_else(|| ParseError::Malformed("structured-response malformed".into()))?;
    let content = after_open[gt + 1..].trim();
    let close = content
        .find(STRUCTURED_CLOSE)
        .ok_or_else(|| ParseError::Malformed("structured-response close not found".into()))?;
    let json_str = content[..close].trim();
    if json_str.is_empty() {
        return Err(ParseError::Malformed(
            "structured-response block is empty — agent must output valid JSON between the tags"
                .into(),
        ));
    }
    let parsed: StructuredGreen =
        serde_json::from_str(json_str).map_err(|e| ParseError::Malformed(e.to_string()))?;

    if parsed.goal.as_deref() != Some("green") {
        return Err(ParseError::Malformed("goal is not green".into()));
    }

    let summary = parsed
        .summary
        .filter(|s| !s.is_empty())
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
        test_command: parsed.test_command.filter(|s| !s.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|s| !s.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|s| !s.is_empty()),
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

/// Parse LLM red goal response from structured-response block.
/// Uses the last block in the output — earlier blocks may be from tool results (e.g. system prompt).
pub fn parse_red_response(s: &str) -> Result<RedOutput, ParseError> {
    let open = s
        .rfind(STRUCTURED_OPEN)
        .ok_or_else(|| ParseError::Malformed("structured-response not found".into()))?;
    let after_open = &s[open + STRUCTURED_OPEN.len()..];
    let gt = after_open
        .find('>')
        .ok_or_else(|| ParseError::Malformed("structured-response malformed".into()))?;
    let content = after_open[gt + 1..].trim();
    let close = content
        .find(STRUCTURED_CLOSE)
        .ok_or_else(|| ParseError::Malformed("structured-response close not found".into()))?;
    let json_str = content[..close].trim();
    if json_str.is_empty() {
        return Err(ParseError::Malformed(
            "structured-response block is empty — agent must output valid JSON between the tags"
                .into(),
        ));
    }
    let parsed: StructuredRed =
        serde_json::from_str(json_str).map_err(|e| ParseError::Malformed(e.to_string()))?;

    if parsed.goal.as_deref() != Some("red") {
        return Err(ParseError::Malformed("goal is not red".into()));
    }

    let summary = parsed
        .summary
        .filter(|s| !s.is_empty())
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

    Ok(RedOutput {
        summary,
        tests,
        skeletons,
        test_command: parsed.test_command.filter(|s| !s.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|s| !s.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|s| !s.is_empty()),
        markers,
        marker_results,
        test_output_file: parsed.test_output_file.filter(|s| !s.is_empty()),
        sequential_command: parsed.sequential_command.filter(|s| !s.is_empty()),
        logging_command: parsed.logging_command.filter(|s| !s.is_empty()),
        metric_hooks: parsed.metric_hooks.filter(|s| !s.is_empty()),
        feedback_options: parsed.feedback_options.filter(|s| !s.is_empty()),
    })
}

/// Parsed validate-changes goal output.
#[derive(Debug, Clone)]
pub struct ValidateOutput {
    pub summary: String,
    pub risk_level: String,
    pub build_results: Vec<ValidateBuildResult>,
    pub issues: Vec<ValidateIssue>,
    pub changeset_sync: Option<ValidateChangesetSync>,
    pub files_analyzed: Vec<ValidateFileAnalyzed>,
    pub test_impact: Option<ValidateTestImpact>,
}

/// Build result entry from validate-changes output.
#[derive(Debug, Clone)]
pub struct ValidateBuildResult {
    pub package: String,
    pub status: String,
    pub notes: Option<String>,
}

/// An issue found during validation.
#[derive(Debug, Clone)]
pub struct ValidateIssue {
    pub severity: String,
    pub category: String,
    pub file: String,
    pub line: Option<u32>,
    pub description: String,
    pub suggestion: Option<String>,
}

/// Changeset sync status from validate-changes output.
#[derive(Debug, Clone)]
pub struct ValidateChangesetSync {
    pub status: String,
    pub items_updated: u32,
    pub items_added: u32,
}

/// File analyzed entry from validate-changes output.
#[derive(Debug, Clone)]
pub struct ValidateFileAnalyzed {
    pub file: String,
    pub lines_changed: Option<u32>,
    pub changeset_item: Option<String>,
}

/// Test impact summary from validate-changes output.
#[derive(Debug, Clone)]
pub struct ValidateTestImpact {
    pub tests_affected: u32,
    pub new_tests_needed: u32,
}

#[derive(serde::Deserialize)]
struct StructuredValidate {
    goal: Option<String>,
    summary: Option<String>,
    risk_level: Option<String>,
    #[serde(default)]
    build_results: Option<Vec<ValidateBuildResultDe>>,
    #[serde(default)]
    issues: Option<Vec<ValidateIssueDe>>,
    #[serde(default)]
    changeset_sync: Option<ValidateChangesetSyncDe>,
    #[serde(default)]
    files_analyzed: Option<Vec<ValidateFileAnalyzedDe>>,
    #[serde(default)]
    test_impact: Option<ValidateTestImpactDe>,
}

#[derive(serde::Deserialize)]
struct ValidateBuildResultDe {
    package: String,
    status: String,
    notes: Option<String>,
}

#[derive(serde::Deserialize)]
struct ValidateIssueDe {
    severity: String,
    category: String,
    file: String,
    line: Option<u32>,
    description: String,
    suggestion: Option<String>,
}

#[derive(serde::Deserialize)]
struct ValidateChangesetSyncDe {
    status: String,
    items_updated: u32,
    items_added: u32,
}

#[derive(serde::Deserialize)]
struct ValidateFileAnalyzedDe {
    file: String,
    lines_changed: Option<u32>,
    changeset_item: Option<String>,
}

#[derive(serde::Deserialize)]
struct ValidateTestImpactDe {
    tests_affected: u32,
    new_tests_needed: u32,
}

/// Parse LLM validate-changes response from structured-response block.
/// Uses the last block (rfind) to skip tool results / system prompt examples that may appear earlier.
/// Returns Malformed if the expected format is not found or goal != "validate-changes".
pub fn parse_validate_response(s: &str) -> Result<ValidateOutput, ParseError> {
    eprintln!(
        r#"{{"tddy":{{"marker_id":"M007","scope":"output::parse_validate_response","data":{{}}}}}}"#
    );
    let open = s
        .rfind(STRUCTURED_OPEN)
        .ok_or_else(|| ParseError::Malformed("structured-response not found".into()))?;
    let after_open = &s[open + STRUCTURED_OPEN.len()..];
    let gt = after_open
        .find('>')
        .ok_or_else(|| ParseError::Malformed("structured-response malformed".into()))?;
    let content = after_open[gt + 1..].trim();
    let close = content
        .find(STRUCTURED_CLOSE)
        .ok_or_else(|| ParseError::Malformed("structured-response close not found".into()))?;
    let json_str = content[..close].trim();
    if json_str.is_empty() {
        return Err(ParseError::Malformed(
            "structured-response block is empty — agent must output valid JSON between the tags"
                .into(),
        ));
    }
    let parsed: StructuredValidate =
        serde_json::from_str(json_str).map_err(|e| ParseError::Malformed(e.to_string()))?;

    if parsed.goal.as_deref() != Some("validate-changes") {
        return Err(ParseError::Malformed("goal is not validate-changes".into()));
    }

    let summary = parsed
        .summary
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "No summary provided.".to_string());

    let risk_level = parsed
        .risk_level
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let build_results = parsed
        .build_results
        .unwrap_or_default()
        .into_iter()
        .map(|b| ValidateBuildResult {
            package: b.package,
            status: b.status,
            notes: b.notes,
        })
        .collect();

    let issues = parsed
        .issues
        .unwrap_or_default()
        .into_iter()
        .map(|i| ValidateIssue {
            severity: i.severity,
            category: i.category,
            file: i.file,
            line: i.line,
            description: i.description,
            suggestion: i.suggestion,
        })
        .collect();

    let changeset_sync = parsed.changeset_sync.map(|c| ValidateChangesetSync {
        status: c.status,
        items_updated: c.items_updated,
        items_added: c.items_added,
    });

    let files_analyzed = parsed
        .files_analyzed
        .unwrap_or_default()
        .into_iter()
        .map(|f| ValidateFileAnalyzed {
            file: f.file,
            lines_changed: f.lines_changed,
            changeset_item: f.changeset_item,
        })
        .collect();

    let test_impact = parsed.test_impact.map(|t| ValidateTestImpact {
        tests_affected: t.tests_affected,
        new_tests_needed: t.new_tests_needed,
    });

    Ok(ValidateOutput {
        summary,
        risk_level,
        build_results,
        issues,
        changeset_sync,
        files_analyzed,
        test_impact,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_prd_and_todo_from_delimited_output() {
        let input = r#"preface
---PRD_START---
# PRD

## Summary
Feature X
---PRD_END---
middle
---TODO_START---
- [ ] Task 1
- [ ] Task 2
---TODO_END---
trailing"#;
        let out = parse_planning_output(input).expect("should parse");
        assert!(out.prd.contains("Feature X"));
        assert!(out.todo.contains("Task 1"));
    }

    #[test]
    fn errors_on_missing_prd() {
        let input = "---TODO_START---\n- [ ] Task\n---TODO_END---";
        let err = parse_planning_output(input).unwrap_err();
        assert!(matches!(err, ParseError::MissingPrd));
    }

    #[test]
    fn errors_on_missing_todo() {
        let input = "---PRD_START---\n# PRD\n---PRD_END---";
        let err = parse_planning_output(input).unwrap_err();
        assert!(matches!(err, ParseError::MissingTodo));
    }

    #[test]
    fn parse_planning_response_returns_planning_output_when_prd_todo_present() {
        let input = r#"preface
---PRD_START---
# PRD

## Summary
Feature X
---PRD_END---
---TODO_START---
- [ ] Task 1
---TODO_END---
trailing"#;
        let out = parse_planning_response(input).expect("should parse");
        assert!(out.prd.contains("Feature X"));
        assert!(out.todo.contains("Task 1"));
    }

    #[test]
    fn parse_planning_response_errors_on_malformed_when_neither_present() {
        let input = "Some random text without delimiters";
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_errors_when_only_questions_delimiters_present() {
        let input = r#"---QUESTIONS_START---
What is the target audience?
---QUESTIONS_END---"#;
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn parse_planning_response_extracts_structured_response() {
        let input = "Here is my analysis.\n\n<structured-response content-type=\"application-json\">\n{\"goal\": \"plan\", \"prd\": \"Summary: Feature X\", \"todo\": \"- [ ] Task 1\"}\n</structured-response>\n\nThat concludes the plan.";
        let out = parse_planning_response(input).expect("should parse");
        assert!(out.prd.contains("Feature X"));
        assert!(out.todo.contains("Task 1"));
    }

    #[test]
    fn parse_planning_response_skips_invalid_block_and_uses_valid_one() {
        let input = r#"System prompt with example (invalid JSON):
<structured-response content-type="application-json">
{"goal": "plan", "prd": "<PRD markdown content>", "todo": 
</structured-response>

Model output:
<structured-response content-type="application-json">
{"goal": "plan", "prd": "Summary: Real PRD", "todo": "- [ ] Real task"}
</structured-response>"#;
        let out = parse_planning_response(input).expect("should parse");
        assert!(out.prd.contains("Real PRD"));
        assert!(out.todo.contains("Real task"));
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
        let input = r#"Created skeleton code.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created 2 skeletons and 1 failing test.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":5,"kind":"struct"},{"name":"bar","file":"src/foo.rs","line":8,"kind":"method"}]}
</structured-response>
"#;
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
    fn parse_red_response_skips_example_block_in_system_prompt() {
        // Simulates Cursor stream: tool results contain system prompt with example block,
        // then assistant/result has the actual agent output. Parser must skip the first
        // (invalid) block and use the second.
        let input = r#"From tool result (red.rs file content):
<structured-response content-type="application-json">
{"goal": "red", "summary": "<human-readable summary>", "tests": [{"name": "<test_name>", "file": "<path>", "line": <number>, "status": "failing"}], "skeletons": []}
</structured-response>

The Red phase skeleton and tests are already in place.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created 2 skeletons and 1 failing test.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":5,"kind":"struct"}]}
</structured-response>
"#;
        let out = super::parse_red_response(input).expect("should parse");
        assert!(out.summary.contains("2 skeletons"));
        assert_eq!(out.skeletons.len(), 1);
        assert_eq!(out.skeletons[0].name, "Foo");
    }

    #[test]
    fn parse_red_response_extracts_test_command_and_prerequisite_actions() {
        let input = r#"Created skeleton code.

<structured-response content-type="application-json">
{"goal":"red","summary":"Created skeletons.","tests":[],"skeletons":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;
        let out = super::parse_red_response(input).expect("should parse");
        assert_eq!(out.test_command.as_deref(), Some("cargo test"));
        assert_eq!(out.prerequisite_actions.as_deref(), Some("None"));
        assert_eq!(
            out.run_single_or_selected_tests.as_deref(),
            Some("cargo test <name>")
        );
    }

    #[test]
    fn parse_acceptance_tests_response_extracts_summary_and_tests() {
        use super::parse_acceptance_tests_response;
        let input = r#"Created acceptance tests.

<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}
</structured-response>
"#;
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
        let input = r#"Created acceptance tests.

<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Created 2 tests.","tests":[{"name":"t1","file":"t.rs","line":1,"status":"failing"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;
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
        let input = r#"Implemented production code.

<structured-response content-type="application-json">
{"goal":"green","summary":"Implemented 2 methods. All tests passing.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"passing"},{"name":"test_bar","file":"src/bar.rs","line":20,"status":"failing","reason":"timeout"}],"implementations":[{"name":"AuthService::validate","file":"src/service.rs","line":15,"kind":"method"}]}
</structured-response>
"#;
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
        let input = r#"Implemented production code.

<structured-response content-type="application-json">
{"goal":"green","summary":"Implemented.","tests":[],"implementations":[],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}
</structured-response>
"#;
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
        let input = r#"<structured-response content-type="application-json">
{"goal":"red","summary":"Wrong goal.","tests":[],"implementations":[]}
</structured-response>"#;
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
