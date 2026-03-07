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
}

#[derive(serde::Deserialize)]
struct StructuredPlan {
    goal: Option<String>,
    prd: Option<String>,
    todo: Option<String>,
}

/// Extract JSON from first <structured-response content-type="application-json">...</structured-response>.
fn extract_structured_response(s: &str) -> Option<PlanningOutput> {
    let open = s.find(STRUCTURED_OPEN)?;
    let after_open = &s[open + STRUCTURED_OPEN.len()..];
    let gt = after_open.find('>')?;
    let content = after_open[gt + 1..].trim();
    let close = content.find(STRUCTURED_CLOSE)?;
    let json_str = content[..close].trim();
    let parsed: StructuredPlan = serde_json::from_str(json_str).ok()?;
    if parsed.goal.as_deref() != Some("plan") {
        return None;
    }
    let prd = parsed.prd.filter(|s| !s.is_empty())?;
    let todo = parsed.todo.filter(|s| !s.is_empty())?;
    Some(PlanningOutput { prd, todo })
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
        "PRD/TODO delimiters not found".into(),
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

    Ok(PlanningOutput { prd, todo })
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
    })
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
pub fn parse_red_response(s: &str) -> Result<RedOutput, ParseError> {
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

    Ok(RedOutput {
        summary,
        tests,
        skeletons,
        test_command: parsed.test_command.filter(|s| !s.is_empty()),
        prerequisite_actions: parsed.prerequisite_actions.filter(|s| !s.is_empty()),
        run_single_or_selected_tests: parsed
            .run_single_or_selected_tests
            .filter(|s| !s.is_empty()),
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
}
