//! Merged post-green submit schema (single `tddy-tools submit` replacing separate evaluate + validate).

use tddy_core::error::ParseError;

/// Parsed merged post-green review output (evaluate + validate concerns in one payload).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PostGreenReviewOutput {
    pub goal: String,
    pub summary: String,
    /// From evaluate-style reporting.
    pub risk_level: String,
    pub validity_assessment: String,
    /// From validate-style reporting.
    pub tests_report_written: bool,
    pub prod_ready_report_written: bool,
    pub clean_code_report_written: bool,
}

/// Parse JSON from `tddy-tools submit` for the merged post-green goal.
pub fn parse_post_green_review_response(s: &str) -> Result<PostGreenReviewOutput, ParseError> {
    log::debug!(
        "parse_post_green_review_response: parsing merged post-green-review payload ({} bytes)",
        s.len()
    );
    let parsed: PostGreenReviewOutput = serde_json::from_str(s).map_err(|e| {
        log::debug!("parse_post_green_review_response: serde error: {e}");
        ParseError::Malformed(format!("post-green-review JSON: {e}"))
    })?;
    if parsed.goal != "post-green-review" {
        log::debug!(
            "parse_post_green_review_response: unexpected goal field {:?}",
            parsed.goal
        );
        return Err(ParseError::Malformed(format!(
            "expected goal \"post-green-review\", got {:?}",
            parsed.goal
        )));
    }
    log::info!(
        "parse_post_green_review_response: ok summary_len={} risk_level={}",
        parsed.summary.len(),
        parsed.risk_level
    );
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = r#"{
  "goal": "post-green-review",
  "summary": "s",
  "risk_level": "low",
  "validity_assessment": "ok",
  "tests_report_written": true,
  "prod_ready_report_written": false,
  "clean_code_report_written": true
}"#;

    #[test]
    fn post_green_review_parser_accepts_minimal_valid_json() {
        let r = parse_post_green_review_response(GOLDEN);
        assert!(
            r.is_ok(),
            "merged post-green submit must parse: {:?}",
            r.err()
        );
    }

    #[test]
    fn post_green_review_parser_rejects_invalid_json() {
        let err = parse_post_green_review_response("not json").expect_err("malformed");
        assert!(matches!(err, ParseError::Malformed(_)));
    }

    #[test]
    fn post_green_review_parser_rejects_wrong_goal_field() {
        let bad = r#"{
  "goal": "evaluate-changes",
  "summary": "x",
  "risk_level": "low",
  "validity_assessment": "ok",
  "tests_report_written": true,
  "prod_ready_report_written": false,
  "clean_code_report_written": true
}"#;
        let r = parse_post_green_review_response(bad);
        assert!(r.is_err());
        let ParseError::Malformed(msg) = r.expect_err("wrong goal") else {
            panic!("expected Malformed");
        };
        assert!(
            msg.contains("post-green-review"),
            "message should mention expected goal: {msg}"
        );
    }
}
