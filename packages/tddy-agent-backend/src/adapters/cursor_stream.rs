//! `cursor-stream`: Cursor Agent's `--output-format stream-json --stream-partial-output`, read by
//! the parser that already reads it (`stream::cursor::process_cursor_stream`) — wrapped, not
//! rewritten (`#agent-tools 9/12`). Named by `cursor.yaml`'s `coding_agent.headless`.

use std::io::BufRead;

use crate::backend::headless_driver::HeadlessHarness;
use crate::error::BackendError;
use crate::stream::StreamResult;

use super::{StreamAdapter, StreamSinks};

/// [`CursorStream`]'s name.
pub const CURSOR_STREAM: &str = "cursor-stream";

/// The [`StreamAdapter`] for Cursor Agent's stream-json output.
#[derive(Debug, Clone, Copy, Default)]
pub struct CursorStream;

impl StreamAdapter for CursorStream {
    fn name(&self) -> &str {
        CURSOR_STREAM
    }

    fn read_stream(
        &self,
        stdout: &mut dyn BufRead,
        sinks: StreamSinks<'_>,
    ) -> Result<StreamResult, BackendError> {
        // TODO(headless-others): implement — `stream::cursor::process_cursor_stream(stdout,
        // sinks.on_progress, sinks.on_raw_output, sinks.on_debug_line, sinks.on_conversation_line,
        // 0)`, its error as `BackendError::InvocationFailed("stream parse error: …")` as
        // `CursorBackend::invoke_sync` maps it today.
        let _ = (stdout, sinks);
        todo!("headless-others: CursorStream::read_stream")
    }

    /// Zero, or `exit_ok_if_structured` and a structured result — Claude's rule; `cursor.yaml`
    /// sets no `exit_ok_if_structured`, so a non-zero exit fails as it does today.
    fn exit_ok(&self, exit_code: i32, stream: &StreamResult, exit_ok_if_structured: bool) -> bool {
        // TODO(headless-others): implement.
        let _ = (exit_code, stream, exit_ok_if_structured);
        todo!("headless-others: CursorStream::exit_ok")
    }

    fn turn_format(&self) -> Option<HeadlessHarness> {
        // TODO(headless-others): implement — the driver's stream-json transcript for Cursor.
        todo!("headless-others: CursorStream::turn_format")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::stream::cursor::process_cursor_stream;
    use crate::stream::ProgressEvent;

    /// A recorded `agent -p … --output-format stream-json --stream-partial-output` run: init, a
    /// tool call and its completion, partial assistant text, and the closing result.
    const A_RECORDED_STREAM: &str = concat!(
        r#"{"type":"system","subtype":"init","session_id":"chat-rec-1","model":"composer-2.5"}"#,
        "\n",
        r#"{"type":"tool_call","subtype":"started","call_id":"call-1","tool_call":{"readToolCall":{"args":{"path":"src/lib.rs"}}},"session_id":"chat-rec-1"}"#,
        "\n",
        r#"{"type":"tool_call","subtype":"completed","call_id":"call-1","tool_call":{"readToolCall":{"args":{"path":"src/lib.rs"},"result":{"success":{"content":"fn main() {}"}}}},"session_id":"chat-rec-1"}"#,
        "\n",
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Reading done."}]},"session_id":"chat-rec-1"}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":false,"result":"The plan is ready.","session_id":"chat-rec-1"}"#,
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

    /// `stream` read by today's parser, as `CursorBackend::invoke_sync` calls it.
    fn read_by_todays_parser(stream: &str) -> AReading {
        let mut progress = Vec::new();
        let mut raw_output = Vec::new();
        let mut conversation_lines = Vec::new();
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());
        let result = process_cursor_stream(
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

    /// `stream` read through the `cursor-stream` adapter.
    fn read_by_the_adapter(stream: &str) -> AReading {
        let mut progress = Vec::new();
        let mut raw_output = Vec::new();
        let mut conversation_lines = Vec::new();
        let mut on_progress = |event: &ProgressEvent| progress.push(format!("{event:?}"));
        let mut on_raw_output = |text: &str| raw_output.push(text.to_string());
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());
        let result = CursorStream
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

    #[test]
    fn the_adapter_reads_a_recorded_cursor_stream_into_the_same_result_and_events_as_todays_parser()
    {
        // Given
        let expected = read_by_todays_parser(A_RECORDED_STREAM);

        // When
        let read = read_by_the_adapter(A_RECORDED_STREAM);

        // Then
        assert_eq!(read, expected);
    }

    #[test]
    fn todays_parser_reads_the_recorded_cursor_stream_into_a_result_and_its_chat_id() {
        // Given — the positive control for the equality above: the recording is not empty

        // When
        let read = read_by_todays_parser(A_RECORDED_STREAM);

        // Then
        assert_eq!(
            (
                read.session_id.as_str(),
                read.result_text.is_empty(),
                read.conversation_lines.len()
            ),
            ("chat-rec-1", false, 5)
        );
    }

    #[test]
    fn a_non_zero_cursor_exit_fails_and_a_zero_one_succeeds() {
        // Given
        let stream = StreamResult {
            result_text: "The plan is ready.".to_string(),
            session_id: "chat-rec-1".to_string(),
            questions: Vec::new(),
            raw_lines: Vec::new(),
            stream_errors: Vec::new(),
        };

        // When
        let ok = (
            CursorStream.exit_ok(0, &stream, false),
            CursorStream.exit_ok(1, &stream, false),
        );

        // Then
        assert_eq!(ok, (true, false));
    }

    #[test]
    fn a_cursor_stream_is_decoded_by_the_drivers_cursor_transcript() {
        // When
        let format = CursorStream.turn_format();

        // Then
        assert_eq!(format, Some(HeadlessHarness::Cursor));
    }
}
