//! Parser for delimited PRD/TODO output from LLM.

use crate::error::ParseError;

const PRD_START: &str = "---PRD_START---";
const PRD_END: &str = "---PRD_END---";
const TODO_START: &str = "---TODO_START---";
const TODO_END: &str = "---TODO_END---";
const QUESTIONS_START: &str = "---QUESTIONS_START---";
const QUESTIONS_END: &str = "---QUESTIONS_END---";

/// Result of parsing a planning response: either questions for clarification or PRD/TODO output.
#[derive(Debug, Clone)]
pub enum PlanningResponse {
    /// Claude asked clarifying questions.
    Questions { questions: Vec<String> },
    /// Claude produced PRD and TODO.
    PlanningOutput { prd: String, todo: String },
}

/// Parsed planning output containing PRD and TODO content.
#[derive(Debug, Clone)]
pub struct PlanningOutput {
    pub prd: String,
    pub todo: String,
}

/// Parse LLM planning response: either QUESTIONS or PRD/TODO. Returns Malformed if neither.
pub fn parse_planning_response(s: &str) -> Result<PlanningResponse, ParseError> {
    if s.contains(QUESTIONS_START) && s.contains(QUESTIONS_END) {
        let content = extract_section(s, QUESTIONS_START, QUESTIONS_END)
            .ok_or_else(|| ParseError::Malformed("questions section incomplete".into()))?;
        let questions: Vec<String> = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        return Ok(PlanningResponse::Questions { questions });
    }
    if s.contains(PRD_START) && s.contains(TODO_START) {
        let out = parse_planning_output(s)?;
        return Ok(PlanningResponse::PlanningOutput {
            prd: out.prd,
            todo: out.todo,
        });
    }
    Err(ParseError::Malformed("neither questions nor PRD/TODO found".into()))
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
    fn parse_planning_response_returns_questions_when_questions_delimiters_present() {
        let input = r#"preface
---QUESTIONS_START---
What is the target audience?
What is the expected timeline?
---QUESTIONS_END---
trailing"#;
        let resp = parse_planning_response(input).expect("should parse");
        match &resp {
            PlanningResponse::Questions { questions } => {
                assert_eq!(questions.len(), 2);
                assert_eq!(questions[0], "What is the target audience?");
                assert_eq!(questions[1], "What is the expected timeline?");
            }
            _ => panic!("expected Questions variant"),
        }
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
        let resp = parse_planning_response(input).expect("should parse");
        match &resp {
            PlanningResponse::PlanningOutput { prd, todo } => {
                assert!(prd.contains("Feature X"));
                assert!(todo.contains("Task 1"));
            }
            _ => panic!("expected PlanningOutput variant"),
        }
    }

    #[test]
    fn parse_planning_response_errors_on_malformed_when_neither_present() {
        let input = "Some random text without delimiters";
        let err = parse_planning_response(input).unwrap_err();
        assert!(matches!(err, ParseError::Malformed(_)));
    }
}
