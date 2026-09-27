//! The structured facts of one tool result — what a caller reads where the raw result's preview
//! is only its first 240 characters.
//!
//! Split out of `subagent.rs` on the oversized-file record
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`): extraction, the fact
//! vocabulary and its tests are a self-contained concern, and the turn loop's append site calls
//! [`summarize`] without carrying the per-tool knowledge itself.
//!
//! Every summary is **bounded** — no unbounded strings. The only text is a `first_line`, itself
//! cut, because a summary that could carry a whole result would cost the outcome's reader the
//! context the preview already spends (`docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`:
//! every descriptor rides the final LiveKit frame, whose whole budget is 48 KiB).

/// The first non-empty line of a `READ` result, cut to keep the summary bounded.
pub const SUMMARY_FIRST_LINE_CHARS: usize = 120;

/// One tool result's facts, as a caller reads them — `camelCase` on the wire, matching the
/// descriptor it rides (`MessageDescriptor`), so the tool description and the payload agree.
///
/// Externally tagged: a `READ` result summarizes as `{"read": {…}}`, a `STR_REPLACE` as
/// `{"strReplace": {…}}` — one key naming the tool, so a caller dispatches on the same name it
/// dispatches the tool call on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ResultSummary {
    /// `READ`: how much came back and of how much.
    Read {
        /// The first non-empty line of the content, cut to [`SUMMARY_FIRST_LINE_CHARS`] —
        /// `None` for an empty file or an all-whitespace one.
        first_line: Option<String>,
        /// How many characters the content carries.
        chars_read: u64,
        /// The file's whole line count, as `total_lines` reports it.
        total_lines: u64,
        /// Whether the content was truncated by a window the model asked for.
        truncated: bool,
    },
    /// `GREP`: how many matches, of how many.
    Grep {
        match_count: u64,
        truncated: bool,
        total_matches: u64,
    },
    /// `GLOB`: how many paths, of how many.
    Glob {
        path_count: u64,
        truncated: bool,
        total_paths: u64,
    },
    /// `STR_REPLACE`: whether it replaced, how much, and the occurrence count the engine
    /// reports (`matchedOccurrences`) as the lines its edit touched.
    StrReplace {
        replaced: bool,
        matched_lines: u64,
        bytes_written: u64,
    },
    /// `WRITE`: how many bytes landed.
    Write { bytes_written: u64 },
    /// `DELETE`: whether the file went.
    Delete { deleted: bool },
    /// `SHELL`: how the command ended — a blocking call's exit code and output size, or the
    /// background job's id.
    Shell {
        exit_code: Option<i64>,
        stdout_chars: Option<u64>,
        job_id: Option<String>,
    },
    /// `AWAIT`: how the awaited job ended.
    Await {
        exit_code: Option<i64>,
        completed: bool,
    },
    /// `READ_LINTS`: how many diagnostics came back.
    ReadLints { lint_count: u64 },
    /// A dispatch that produced no result — a failure, a rejection or a repeat. The reason is
    /// the tool result's own text; `is_error` on the descriptor remains the authoritative flag.
    Error,
}

/// The facts of `tool`'s result, read from the result JSON the dispatch produced.
///
/// Called at the one place a tool result is appended to the transcript (`run_one_turn`), where
/// the `ToolDispatch::Ran(value)` JSON is in hand before it is stringified into the tool
/// message — the only place the structure is still visible.
pub fn summarize(_tool: &str, _result: &serde_json::Value) -> ResultSummary {
    // TODO(tool-previews): extract each tool's facts from its result JSON.
    ResultSummary::Error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_summary_reports_its_first_line_char_count_and_totals() {
        let result = serde_json::json!({
            "content": "fn main() {\n    println!(\"hi\");\n}",
            "truncated": false,
            "total_lines": 3
        });
        assert_eq!(
            summarize("READ", &result),
            ResultSummary::Read {
                first_line: Some("fn main() {".to_string()),
                chars_read: 31,
                total_lines: 3,
                truncated: false
            }
        );
    }

    #[test]
    fn a_read_summary_of_an_empty_file_reports_no_first_line() {
        let result = serde_json::json!({ "content": "", "truncated": false, "total_lines": 0 });
        assert_eq!(
            summarize("READ", &result),
            ResultSummary::Read {
                first_line: None,
                chars_read: 0,
                total_lines: 0,
                truncated: false
            }
        );
    }

    #[test]
    fn a_grep_summary_reports_its_match_counts() {
        let result = serde_json::json!({
            "matches": [{"path": "src/lib.rs"}],
            "truncated": true,
            "total_matches": 12
        });
        assert_eq!(
            summarize("GREP", &result),
            ResultSummary::Grep {
                match_count: 1,
                truncated: true,
                total_matches: 12
            }
        );
    }

    #[test]
    fn a_glob_summary_reports_its_path_counts() {
        let result = serde_json::json!({
            "paths": ["src/lib.rs", "src/main.rs"],
            "truncated": false,
            "total_paths": 2
        });
        assert_eq!(
            summarize("GLOB", &result),
            ResultSummary::Glob {
                path_count: 2,
                truncated: false,
                total_paths: 2
            }
        );
    }

    #[test]
    fn a_str_replace_summary_reports_its_replacement_and_occurrences() {
        let result = serde_json::json!({
            "replaced": true,
            "matchedOccurrences": 1,
            "bytes_written": 128
        });
        assert_eq!(
            summarize("STR_REPLACE", &result),
            ResultSummary::StrReplace {
                replaced: true,
                matched_lines: 1,
                bytes_written: 128
            }
        );
    }

    #[test]
    fn a_write_summary_reports_its_bytes_written() {
        let result = serde_json::json!({ "bytes_written": 4096 });
        assert_eq!(
            summarize("WRITE", &result),
            ResultSummary::Write {
                bytes_written: 4096
            }
        );
    }

    #[test]
    fn a_delete_summary_reports_its_deletion() {
        let result = serde_json::json!({ "deleted": true });
        assert_eq!(
            summarize("DELETE", &result),
            ResultSummary::Delete { deleted: true }
        );
    }

    #[test]
    fn a_blocking_shell_summary_reports_its_exit_code_and_output_size() {
        let result = serde_json::json!({ "stdout": "built\n", "stderr": "", "exit_code": 0 });
        assert_eq!(
            summarize("SHELL", &result),
            ResultSummary::Shell {
                exit_code: Some(0),
                stdout_chars: Some(6),
                job_id: None
            }
        );
    }

    #[test]
    fn a_background_shell_summary_reports_its_job_id() {
        let result = serde_json::json!({ "job_id": "job-7" });
        assert_eq!(
            summarize("SHELL", &result),
            ResultSummary::Shell {
                exit_code: None,
                stdout_chars: None,
                job_id: Some("job-7".to_string())
            }
        );
    }

    #[test]
    fn an_await_summary_reports_its_completion_and_exit_code() {
        let result = serde_json::json!({ "stdout": "done\n", "exit_code": 0, "completed": true });
        assert_eq!(
            summarize("AWAIT", &result),
            ResultSummary::Await {
                exit_code: Some(0),
                completed: true
            }
        );
    }

    #[test]
    fn a_read_lints_summary_reports_its_diagnostic_count() {
        let result = serde_json::json!({ "lints": [{"line": 1}, {"line": 2}] });
        assert_eq!(
            summarize("READ_LINTS", &result),
            ResultSummary::ReadLints { lint_count: 2 }
        );
    }

    #[test]
    fn a_dispatch_that_produced_nothing_summarizes_as_an_error_shape() {
        let result = serde_json::json!({ "error": "the jail is closed" });
        assert_eq!(summarize("READ", &result), ResultSummary::Error);
    }

    #[test]
    fn a_summary_serializes_as_one_key_naming_its_tool() {
        let summary = ResultSummary::Read {
            first_line: Some("fn main() {".to_string()),
            chars_read: 31,
            total_lines: 3,
            truncated: false,
        };
        let wire = serde_json::to_value(&summary).expect("serializable");
        assert_eq!(wire["read"]["charsRead"], 31);
        // And parses back into the same summary — the shape the string-typed proto field and
        // the MCP outcome both carry as text.
        assert_eq!(
            serde_json::from_value::<ResultSummary>(wire).expect("parseable"),
            summary
        );
    }
}
