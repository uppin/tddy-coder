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
}

/// Info about a single acceptance test.
#[derive(Debug, Clone)]
pub struct AcceptanceTestInfo {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub status: String,
}

#[derive(serde::Deserialize)]
struct StructuredAcceptanceTests {
    goal: Option<String>,
    summary: Option<String>,
    tests: Option<Vec<AcceptanceTestInfoDe>>,
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

    Ok(AcceptanceTestsOutput { summary, tests })
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
}
