//! Post-processors for terminal action output (`result_kind`).

use serde_json::{json, Value};

/// Apply a `result_kind` post-processor to a base invocation record.
pub fn apply_result_kind(
    result_kind: Option<&str>,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
) -> Value {
    let mut record = json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    if result_kind == Some("test_summary") {
        if let Some(summary) = parse_cargo_test_summary(stdout, stderr) {
            record["summary"] = summary;
        }
    }
    record
}

fn parse_cargo_test_summary(stdout: &str, stderr: &str) -> Option<Value> {
    let combined = format!("{stdout}{stderr}");
    for line in combined.lines() {
        if let Some(rest) = line.strip_prefix("test result:") {
            let passed = extract_count_before(rest, "passed");
            let failed = extract_count_before(rest, "failed");
            let skipped = extract_count_before(rest, "ignored");
            return Some(json!({ "passed": passed, "failed": failed, "skipped": skipped }));
        }
    }
    None
}

fn extract_count_before(text: &str, label: &str) -> u64 {
    for token in text.split_whitespace() {
        if token.starts_with(label) {
            return 0;
        }
        if let Ok(n) = token.parse::<u64>() {
            // peek next — if next token starts with label, this is our count
            let after = text.split_once(token).map(|(_, r)| r).unwrap_or("");
            if after.trim_start().starts_with(label) {
                return n;
            }
        }
    }
    // fallback: "2 passed;" pattern
    if let Some(idx) = text.find(label) {
        let before = text[..idx].trim();
        if let Some(num) = before.split_whitespace().next_back() {
            return num.parse().unwrap_or(0);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_extracts_cargo_totals() {
        // Given
        let stdout = "running 2 tests\ntest result: ok. 2 passed; 0 failed; 0 ignored;";

        // When
        let record = apply_result_kind(Some("test_summary"), stdout, "", 0);

        // Then
        assert_eq!(record["summary"]["passed"], 2);
        assert_eq!(record["summary"]["failed"], 0);
    }
}
