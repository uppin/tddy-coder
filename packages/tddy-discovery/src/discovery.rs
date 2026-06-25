//! Citation line parser → `DiscoveryData` mapping.
//!
//! FastContext's `<final_answer>` block contains `path:line-start-line-end` citation lines.
//! This module parses them into `RelevantCode{path, reason}` entries for `DiscoveryData`.
//! Malformed lines are excluded (no panic, no fallback that includes garbage).
//!
//! Implemented: `citation_lines_to_discovery_data` and `extract_final_answer`.

use tddy_core::changeset::{DiscoveryData, RelevantCode};

/// Parse `path:N-M` citation lines from a `<final_answer>` block into a `DiscoveryData`.
///
/// Lines that do not match the `path:N-M` format are silently excluded.
/// The `reason` field is set to the line itself (the full citation string).
pub fn citation_lines_to_discovery_data(final_answer: &str) -> DiscoveryData {
    // A valid citation: "<path>:<digits>-<digits>" — path must not be empty, both numbers present.
    let relevant_code: Vec<RelevantCode> = final_answer
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // Find the last colon to split path from line-range.
            let colon_pos = line.rfind(':')?;
            let path = &line[..colon_pos];
            let range = &line[colon_pos + 1..];
            if path.is_empty() {
                return None;
            }
            // Validate range is digits-digits
            let (start, end) = range.split_once('-')?;
            if start.trim().parse::<u64>().is_err() || end.trim().parse::<u64>().is_err() {
                return None;
            }
            Some(RelevantCode {
                path: path.to_string(),
                reason: line.to_string(),
            })
        })
        .collect();

    DiscoveryData {
        toolchain: Default::default(),
        scripts: Default::default(),
        doc_locations: Vec::new(),
        relevant_code,
        test_infrastructure: None,
    }
}

/// Extract the content inside `<final_answer>…</final_answer>` tags.
/// Returns `None` when no tags are present.
pub fn extract_final_answer(model_output: &str) -> Option<&str> {
    const OPEN: &str = "<final_answer>";
    const CLOSE: &str = "</final_answer>";
    let start = model_output.find(OPEN)? + OPEN.len();
    let end = model_output[start..].find(CLOSE)? + start;
    Some(model_output[start..end].trim())
}

#[cfg(test)]
mod tests {
    //! Unit tests: citation line parsing → `DiscoveryData` mapping.
    //!
    //! Feature: docs/ft/coder/discovery-agent.md (Phase C criterion 12)
    //! Changeset: docs/dev/1-WIP/2026-06-24-changeset-fastcontext-discovery.md

    use super::*;

    /// `path:N-M` citation lines are mapped to `RelevantCode{path, reason}` entries.
    #[test]
    fn maps_citation_lines_into_relevant_code_entries() {
        // Given — a final_answer block with two valid citation lines
        let final_answer = "src/auth.rs:1-50\nsrc/auth/mod.rs:10-30";

        // When
        let data = citation_lines_to_discovery_data(final_answer);

        // Then — both citations are present in relevant_code
        assert_eq!(
            data.relevant_code.len(),
            2,
            "two valid citation lines must produce two RelevantCode entries; got {:?}",
            data.relevant_code
        );
        assert!(
            data.relevant_code.iter().any(|rc| rc.path == "src/auth.rs"),
            "src/auth.rs must be in relevant_code; got {:?}",
            data.relevant_code
        );
        assert!(
            data.relevant_code
                .iter()
                .any(|rc| rc.path == "src/auth/mod.rs"),
            "src/auth/mod.rs must be in relevant_code; got {:?}",
            data.relevant_code
        );
        // reason is set to the citation string
        assert!(
            data.relevant_code
                .iter()
                .any(|rc| rc.path == "src/auth.rs" && rc.reason.contains("1-50")),
            "reason must encode the line range; got {:?}",
            data.relevant_code
        );
    }

    /// Malformed lines (no line range, empty, random text) are excluded without panicking.
    #[test]
    fn ignores_malformed_citation_lines() {
        // Given — a mix of valid and malformed lines
        let final_answer = "\
            src/lib.rs:5-20\n\
            not-a-citation\n\
            \n\
            also not valid: no colon\n\
            src/main.rs:100-200\
        ";

        // When
        let data = citation_lines_to_discovery_data(final_answer);

        // Then — only the two valid citations are included; no panic
        assert_eq!(
            data.relevant_code.len(),
            2,
            "only valid citations must be included; got {:?}",
            data.relevant_code
        );
        assert!(
            data.relevant_code
                .iter()
                .all(|rc| rc.path == "src/lib.rs" || rc.path == "src/main.rs"),
            "only src/lib.rs and src/main.rs must be present; got {:?}",
            data.relevant_code
        );
    }

    /// A full model output with `<final_answer>` tags is correctly parsed.
    #[test]
    fn populates_discovery_data_fields_from_final_answer() {
        // Given — model output wrapping citations in <final_answer> tags
        let model_output = "\
            I examined the codebase and found the following relevant files.\n\
            <final_answer>\n\
            src/auth.rs:1-80\n\
            src/session.rs:10-60\n\
            </final_answer>\
        ";

        // When — extract the final answer block and parse it
        let answer = extract_final_answer(model_output)
            .expect("<final_answer> block must be extractable when present");
        let data = citation_lines_to_discovery_data(answer);

        // Then
        assert_eq!(
            data.relevant_code.len(),
            2,
            "two citations inside <final_answer> must produce two entries; got {:?}",
            data.relevant_code
        );
        assert!(
            data.relevant_code.iter().any(|rc| rc.path == "src/auth.rs"),
            "src/auth.rs must be in relevant_code; got {:?}",
            data.relevant_code
        );
        assert!(
            data.relevant_code
                .iter()
                .any(|rc| rc.path == "src/session.rs"),
            "src/session.rs must be in relevant_code; got {:?}",
            data.relevant_code
        );
    }
}
