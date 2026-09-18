//! The `acceptance-tests` phase's parser: `AcceptanceTestsOutput`, `AcceptanceTestInfo` and
//! `parse_acceptance_tests_response`.

use tddy_core::ParseError;

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
    pub(crate) goal: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) tests: Option<Vec<AcceptanceTestInfoDe>>,
    pub(crate) test_command: Option<String>,
    pub(crate) prerequisite_actions: Option<String>,
    pub(crate) run_single_or_selected_tests: Option<String>,
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
struct AcceptanceTestInfoDe {
    pub(crate) name: String,
    pub(crate) file: String,
    pub(crate) line: Option<u32>,
    pub(crate) status: String,
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
