//! `claude-stream-json`: Claude Code's `--output-format stream-json --verbose`, read by the parser
//! that already reads it (`stream::process_ndjson_stream`) — wrapped, not rewritten.

use std::io::BufRead;

use crate::backend::headless_driver::HeadlessHarness;
use crate::error::BackendError;
use crate::stream::StreamResult;

use super::{StreamAdapter, StreamSinks, CLAUDE_STREAM_JSON};

/// The marker a structured result carries; a non-zero exit with it is a success when the spec says
/// so (the CLI can exit 1 after an `ExitPlanMode` despite a complete result).
pub(super) const STRUCTURED_RESPONSE_MARKER: &str = "<structured-response";

/// The [`StreamAdapter`] for Claude Code's stream-json output.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeStreamJson;

impl StreamAdapter for ClaudeStreamJson {
    fn name(&self) -> &str {
        CLAUDE_STREAM_JSON
    }

    fn read_stream(
        &self,
        stdout: &mut dyn BufRead,
        sinks: StreamSinks<'_>,
    ) -> Result<StreamResult, BackendError> {
        // TODO(claude-headless): implement — `stream::process_ndjson_stream(stdout, sinks.on_progress,
        // sinks.on_raw_output, sinks.on_debug_line, sinks.on_conversation_line, 0)`, its error as
        // `BackendError::InvocationFailed("stream parse error: …")` as `invoke_sync` maps it today.
        let _ = (stdout, sinks);
        todo!("claude-headless: ClaudeStreamJson::read_stream")
    }

    fn exit_ok(&self, exit_code: i32, stream: &StreamResult, exit_ok_if_structured: bool) -> bool {
        // TODO(claude-headless): implement — zero, or `exit_ok_if_structured` and the result text
        // carries `STRUCTURED_RESPONSE_MARKER`.
        let _ = (
            exit_code,
            stream,
            exit_ok_if_structured,
            STRUCTURED_RESPONSE_MARKER,
        );
        todo!("claude-headless: ClaudeStreamJson::exit_ok")
    }

    fn turn_format(&self) -> Option<HeadlessHarness> {
        // TODO(claude-headless): implement — the driver's stream-json transcript for Claude.
        todo!("claude-headless: ClaudeStreamJson::turn_format")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::stream::{process_ndjson_stream, ProgressEvent};

    /// A recorded `claude -p … --output-format stream-json --verbose` run: init, a tool use and its
    /// result, assistant text, an `AskUserQuestion`, and the closing result event.
    const A_RECORDED_STREAM: &str = concat!(
        r#"{"type":"system","subtype":"init","session_id":"sess-rec-1","model":"opus"}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"src/lib.rs"}}]},"session_id":"sess-rec-1"}"#,
        "\n",
        r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"fn main() {}"}]},"session_id":"sess-rec-1"}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Reading done."}]},"session_id":"sess-rec-1"}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"AskUserQuestion","input":{"questions":[{"question":"Which parser?","header":"Parser","options":[{"label":"serde","description":"Use serde"}],"multiSelect":false}]}}]},"session_id":"sess-rec-1"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"sess-rec-1"}"#,
        "\n",
    );

    /// What one reading of a stream produced: the result's fields and every callback's arguments.
    #[derive(Debug, PartialEq, Eq)]
    struct AReading {
        result_text: String,
        session_id: String,
        questions: Vec<String>,
        raw_lines: Vec<String>,
        stream_errors: Vec<String>,
        progress: Vec<String>,
        raw_output: Vec<String>,
        conversation_lines: Vec<String>,
    }

    fn a_reading(
        result: StreamResult,
        progress: Vec<String>,
        raw_output: Vec<String>,
        conversation_lines: Vec<String>,
    ) -> AReading {
        AReading {
            result_text: result.result_text,
            session_id: result.session_id,
            questions: result.questions.iter().map(|q| format!("{q:?}")).collect(),
            raw_lines: result.raw_lines,
            stream_errors: result.stream_errors,
            progress,
            raw_output,
            conversation_lines,
        }
    }

    /// `stream` read by today's parser, as `ClaudeCodeBackend::invoke_sync` calls it.
    fn read_by_todays_parser(stream: &str) -> AReading {
        let mut progress = Vec::new();
        let mut raw_output = Vec::new();
        let mut conversation_lines = Vec::new();
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());
        let result = process_ndjson_stream(
            Cursor::new(stream.as_bytes()),
            |event: &ProgressEvent| progress.push(format!("{event:?}")),
            |text: &str| raw_output.push(text.to_string()),
            None,
            Some(&mut on_conversation_line),
            0,
        )
        .expect("today's parser reads the recorded stream");
        a_reading(result, progress, raw_output, conversation_lines)
    }

    /// `stream` read through the `claude-stream-json` adapter.
    fn read_by_the_adapter(stream: &str) -> AReading {
        let mut progress = Vec::new();
        let mut raw_output = Vec::new();
        let mut conversation_lines = Vec::new();
        let mut on_progress = |event: &ProgressEvent| progress.push(format!("{event:?}"));
        let mut on_raw_output = |text: &str| raw_output.push(text.to_string());
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());
        let result = ClaudeStreamJson
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
        a_reading(result, progress, raw_output, conversation_lines)
    }

    fn a_stream_result(result_text: &str) -> StreamResult {
        StreamResult {
            result_text: result_text.to_string(),
            session_id: "sess-rec-1".to_string(),
            questions: Vec::new(),
            raw_lines: Vec::new(),
            stream_errors: Vec::new(),
        }
    }

    #[test]
    fn the_adapter_reads_a_recorded_stream_into_the_same_result_and_events_as_todays_parser() {
        // Given
        let expected = read_by_todays_parser(A_RECORDED_STREAM);

        // When
        let read = read_by_the_adapter(A_RECORDED_STREAM);

        // Then
        assert_eq!(read, expected);
    }

    #[test]
    fn todays_parser_reads_the_recorded_stream_into_a_result_a_session_a_question_and_progress() {
        // Given — the positive control for the equality above: the recording is not empty of events

        // When
        let read = read_by_todays_parser(A_RECORDED_STREAM);

        // Then
        assert_eq!(
            (
                read.result_text.as_str(),
                read.session_id.as_str(),
                read.questions.len(),
                read.progress.is_empty(),
                read.conversation_lines.len(),
            ),
            // Today's parser keeps the assistant text ahead of the result event's text.
            ("Reading done.The plan is ready.", "sess-rec-1", 1, false, 6)
        );
    }

    #[test]
    fn a_zero_exit_is_ok() {
        // Given
        let stream = a_stream_result("done");

        // When
        let ok = ClaudeStreamJson.exit_ok(0, &stream, false);

        // Then
        assert!(ok);
    }

    #[test]
    fn a_non_zero_exit_with_a_structured_result_is_ok_when_the_spec_allows_it() {
        // Given
        let stream = a_stream_result("<structured-response goal=\"plan\">{}</structured-response>");

        // When
        let ok = ClaudeStreamJson.exit_ok(1, &stream, true);

        // Then
        assert!(ok);
    }

    #[test]
    fn a_non_zero_exit_with_a_structured_result_fails_when_the_spec_does_not_allow_it() {
        // Given
        let stream = a_stream_result("<structured-response goal=\"plan\">{}</structured-response>");

        // When
        let ok = ClaudeStreamJson.exit_ok(1, &stream, false);

        // Then
        assert!(!ok);
    }

    #[test]
    fn a_non_zero_exit_without_a_structured_result_fails_even_when_the_spec_allows_it() {
        // Given
        let stream = a_stream_result("I could not finish.");

        // When
        let ok = ClaudeStreamJson.exit_ok(1, &stream, true);

        // Then
        assert!(!ok);
    }

    #[test]
    fn a_claude_stream_is_decoded_by_the_drivers_claude_transcript() {
        // Given
        let adapter = ClaudeStreamJson;

        // When
        let format = adapter.turn_format();

        // Then
        assert_eq!(format, Some(HeadlessHarness::Claude));
    }
}
