//! `codex-jsonl`: Codex's `exec --json` JSONL, read by the parser that already reads it
//! (`stream::codex::parse_codex_jsonl_output`) — wrapped, not rewritten (`#agent-tools 9/12`).
//! Named by `codex.yaml`'s `coding_agent.headless`.

use std::io::BufRead;

use crate::backend::headless_driver::HeadlessHarness;
use crate::error::BackendError;
use crate::stream::StreamResult;

use super::{StreamAdapter, StreamSinks};

/// [`CodexJsonl`]'s name.
pub const CODEX_JSONL: &str = "codex-jsonl";

/// The [`StreamAdapter`] for Codex's JSONL output.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexJsonl;

impl StreamAdapter for CodexJsonl {
    fn name(&self) -> &str {
        CODEX_JSONL
    }

    /// Every line, as `CodexBackend::invoke_sync` reads them today: each handed to the
    /// conversation and debug sinks as it arrives, then the whole read by
    /// `parse_codex_jsonl_output` — its text the result, its thread id the session (empty when
    /// none), the lines kept raw, and `codex_jsonl_last_error_message` the one stream error.
    /// Codex asks no questions and reports no progress on this path.
    fn read_stream(
        &self,
        stdout: &mut dyn BufRead,
        sinks: StreamSinks<'_>,
    ) -> Result<StreamResult, BackendError> {
        // TODO(headless-others): implement.
        let _ = (stdout, sinks);
        todo!("headless-others: CodexJsonl::read_stream")
    }

    /// Zero, or `exit_ok_if_structured` and a non-empty result — what the backend tolerates for a
    /// planning goal today (`claude_nonzero_exit_ok_if_structured_response` and parsed output).
    fn exit_ok(&self, exit_code: i32, stream: &StreamResult, exit_ok_if_structured: bool) -> bool {
        // TODO(headless-others): implement.
        let _ = (exit_code, stream, exit_ok_if_structured);
        todo!("headless-others: CodexJsonl::exit_ok")
    }

    fn turn_format(&self) -> Option<HeadlessHarness> {
        // TODO(headless-others): implement — the driver's Codex transcript.
        todo!("headless-others: CodexJsonl::turn_format")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::stream::codex::{codex_jsonl_last_error_message, parse_codex_jsonl_output};
    use crate::stream::ProgressEvent;

    /// A recorded `codex exec --json` run: the thread, a reasoning item, the agent's message, and
    /// the turn's completion.
    const A_RECORDED_STREAM: &str = concat!(
        r#"{"type":"thread.started","thread_id":"th-rec-1"}"#,
        "\n",
        r#"{"type":"turn.started"}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Reading the parser."}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"The plan is ready."}}"#,
        "\n",
        r#"{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}"#,
        "\n",
    );

    /// A recorded run that failed signing in.
    const A_FAILED_STREAM: &str = concat!(
        r#"{"type":"thread.started","thread_id":"th-rec-2"}"#,
        "\n",
        r#"{"type":"error","message":"unexpected status 401 Unauthorized: Missing bearer or basic authentication"}"#,
        "\n",
        r#"{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: Missing bearer or basic authentication"}}"#,
        "\n",
    );

    fn lines_of(stream: &str) -> Vec<String> {
        stream.lines().map(str::to_string).collect()
    }

    /// What the adapter read, and the lines its conversation sink was handed.
    fn read_by_the_adapter(stream: &str) -> (StreamResult, Vec<String>) {
        let mut conversation_lines = Vec::new();
        let mut on_progress = |_: &ProgressEvent| {};
        let mut on_raw_output = |_: &str| {};
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());
        let result = CodexJsonl
            .read_stream(
                &mut Cursor::new(stream.as_bytes()),
                StreamSinks {
                    on_progress: &mut on_progress,
                    on_raw_output: &mut on_raw_output,
                    on_debug_line: None,
                    on_conversation_line: Some(&mut on_conversation_line),
                },
            )
            .expect("the adapter reads the recorded stream");
        (result, conversation_lines)
    }

    #[test]
    fn the_adapter_reads_a_recorded_codex_stream_into_todays_parsed_text_and_thread_id() {
        // Given
        let today = parse_codex_jsonl_output(&lines_of(A_RECORDED_STREAM));

        // When
        let (read, conversation_lines) = read_by_the_adapter(A_RECORDED_STREAM);

        // Then
        assert_eq!(
            (
                read.result_text,
                Some(read.session_id),
                read.raw_lines,
                read.stream_errors,
                conversation_lines,
            ),
            (
                today.result_text,
                today.session_id,
                lines_of(A_RECORDED_STREAM),
                Vec::<String>::new(),
                lines_of(A_RECORDED_STREAM),
            )
        );
    }

    #[test]
    fn todays_parser_reads_the_recorded_codex_stream_into_the_agents_message_and_thread() {
        // Given — the positive control for the equality above

        // When
        let today = parse_codex_jsonl_output(&lines_of(A_RECORDED_STREAM));

        // Then
        assert_eq!(
            (today.result_text.is_empty(), today.session_id.as_deref()),
            (false, Some("th-rec-1"))
        );
    }

    #[test]
    fn a_failed_codex_stream_carries_todays_last_error_message_as_its_stream_error() {
        // Given
        let today = codex_jsonl_last_error_message(&lines_of(A_FAILED_STREAM));

        // When
        let (read, _) = read_by_the_adapter(A_FAILED_STREAM);

        // Then
        assert_eq!(
            read.stream_errors,
            vec![today.expect("today's parser finds the error")]
        );
    }

    fn a_stream_result(result_text: &str) -> StreamResult {
        StreamResult {
            result_text: result_text.to_string(),
            session_id: "th-rec-1".to_string(),
            questions: Vec::new(),
            raw_lines: Vec::new(),
            stream_errors: Vec::new(),
        }
    }

    #[test]
    fn a_non_zero_codex_exit_with_parsed_output_is_ok_only_when_the_spec_allows_it() {
        // When
        let ok = (
            CodexJsonl.exit_ok(0, &a_stream_result(""), false),
            CodexJsonl.exit_ok(1, &a_stream_result("The plan is ready."), true),
            CodexJsonl.exit_ok(1, &a_stream_result("The plan is ready."), false),
            CodexJsonl.exit_ok(1, &a_stream_result("  "), true),
        );

        // Then
        assert_eq!(ok, (true, true, false, false));
    }

    #[test]
    fn a_codex_stream_is_decoded_by_the_drivers_codex_transcript() {
        // When
        let format = CodexJsonl.turn_format();

        // Then
        assert_eq!(format, Some(HeadlessHarness::Codex));
    }
}
