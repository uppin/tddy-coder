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
///
/// The facts come from the tool the model **called**, not from the shape the result happens to
/// wear: a result without the fields this tool reads summarizes as a summary of empty facts
/// rather than an error, because "no matches found" is a result too. A result carrying an
/// `"error"` key is the one thing that summarizes as [`ResultSummary::Error`] — that key is the
/// pinned shape of a dispatch that produced nothing to report facts about.
pub fn summarize(tool: &str, result: &serde_json::Value) -> ResultSummary {
    if result.get("error").is_some() {
        return ResultSummary::Error;
    }
    match tool {
        "READ" => ResultSummary::Read {
            first_line: first_non_empty_line(str_at(result, "content").unwrap_or_default()),
            chars_read: chars_excluding_newlines(str_at(result, "content").unwrap_or_default()),
            total_lines: u64_at(result, "total_lines"),
            truncated: bool_at(result, "truncated"),
        },
        "GREP" => ResultSummary::Grep {
            match_count: len_at(result, "matches"),
            truncated: bool_at(result, "truncated"),
            total_matches: u64_at(result, "total_matches"),
        },
        "GLOB" => ResultSummary::Glob {
            path_count: len_at(result, "paths"),
            truncated: bool_at(result, "truncated"),
            total_paths: u64_at(result, "total_paths"),
        },
        // The engine spells the occurrence count `matchedOccurrences` on its result — the one
        // field read under its wire name rather than the summary's own.
        "STR_REPLACE" => ResultSummary::StrReplace {
            replaced: bool_at(result, "replaced"),
            matched_lines: u64_at(result, "matchedOccurrences"),
            bytes_written: u64_at(result, "bytes_written"),
        },
        "WRITE" => ResultSummary::Write {
            bytes_written: u64_at(result, "bytes_written"),
        },
        "DELETE" => ResultSummary::Delete {
            deleted: bool_at(result, "deleted"),
        },
        "SHELL" => shell_summary(result),
        "AWAIT" => ResultSummary::Await {
            exit_code: result.get("exit_code").and_then(serde_json::Value::as_i64),
            completed: bool_at(result, "completed"),
        },
        "READ_LINTS" => ResultSummary::ReadLints {
            lint_count: len_at(result, "lints"),
        },
        // A tool name this summary has no facts for: refusing to guess keeps an unrecognized
        // result from being read as, say, a zero-match search.
        _ => ResultSummary::Error,
    }
}

/// A `SHELL` result's facts. A background dispatch has no ending to report — the job id is its
/// whole story, so the exit code and output size stay unset rather than reading as zero.
fn shell_summary(result: &serde_json::Value) -> ResultSummary {
    match str_at(result, "job_id") {
        Some(job_id) => ResultSummary::Shell {
            exit_code: None,
            stdout_chars: None,
            job_id: Some(job_id.to_string()),
        },
        None => ResultSummary::Shell {
            exit_code: result.get("exit_code").and_then(serde_json::Value::as_i64),
            stdout_chars: Some(str_at(result, "stdout").unwrap_or_default().chars().count() as u64),
            job_id: None,
        },
    }
}

/// The value at `key` as a `u64`, or 0 when it is absent or of another shape.
fn u64_at(result: &serde_json::Value, key: &str) -> u64 {
    result
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// The value at `key` as a `bool`, or `false` when it is absent or of another shape.
fn bool_at(result: &serde_json::Value, key: &str) -> bool {
    result
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// How many elements the array at `key` holds, or 0 when it is absent or not an array.
fn len_at(result: &serde_json::Value, key: &str) -> u64 {
    result
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map_or(0, |items| items.len() as u64)
}

/// The value at `key` as a string slice, or `None` when it is absent or of another shape.
fn str_at<'a>(result: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    result.get(key).and_then(serde_json::Value::as_str)
}

/// A text's first line that carries anything, cut to [`SUMMARY_FIRST_LINE_CHARS`].
///
/// `None` when every line is empty or whitespace — the summary of a file with nothing to show.
fn first_non_empty_line(text: &str) -> Option<String> {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.chars().take(SUMMARY_FIRST_LINE_CHARS).collect())
}

/// How many characters a text carries, counting the characters of text and not the newlines that
/// separate its lines — the newlines are structure the `total_lines` field already reports, so an
/// empty file reads as 0 and a one-line file as its length, newline excluded.
fn chars_excluding_newlines(text: &str) -> u64 {
    text.lines().map(|line| line.chars().count() as u64).sum()
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
