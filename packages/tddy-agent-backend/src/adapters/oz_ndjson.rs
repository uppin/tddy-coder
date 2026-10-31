//! `oz-ndjson`: Warp's `oz agent run --output-format ndjson`, read by the transcript that already
//! reads it (`headless_driver/oz_ndjson.rs`) — wrapped, not rewritten (`#agent-tools 9/12`).
//! Named by `oz.yaml`'s `coding_agent.headless`.

use std::io::BufRead;

use crate::backend::headless_driver::HeadlessHarness;
use crate::error::BackendError;
use crate::stream::StreamResult;

use super::{StreamAdapter, StreamSinks};

/// [`OzNdjson`]'s name.
pub const OZ_NDJSON: &str = "oz-ndjson";

/// The [`StreamAdapter`] for `oz`'s NDJSON output.
#[derive(Debug, Clone, Copy, Default)]
pub struct OzNdjson;

impl StreamAdapter for OzNdjson {
    fn name(&self) -> &str {
        OZ_NDJSON
    }

    /// Every line through the driver's Oz transcript with no update sink: the `agent` events'
    /// text the result, the `conversation_started` id the session (empty when none), the lines
    /// kept raw and handed to the conversation and debug sinks. No workflow backend runs `oz`
    /// today; this is the same reading the driver does, for a consumer that is not a conversation.
    fn read_stream(
        &self,
        stdout: &mut dyn BufRead,
        sinks: StreamSinks<'_>,
    ) -> Result<StreamResult, BackendError> {
        // TODO(headless-others): implement.
        let _ = (stdout, sinks);
        todo!("headless-others: OzNdjson::read_stream")
    }

    /// Zero only: an `oz` run has no structured result.
    fn exit_ok(&self, exit_code: i32, stream: &StreamResult, exit_ok_if_structured: bool) -> bool {
        // TODO(headless-others): implement.
        let _ = (exit_code, stream, exit_ok_if_structured);
        todo!("headless-others: OzNdjson::exit_ok")
    }

    fn turn_format(&self) -> Option<HeadlessHarness> {
        // TODO(headless-others): implement — the driver's Oz transcript.
        todo!("headless-others: OzNdjson::turn_format")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::stream::ProgressEvent;

    /// A recorded `oz agent run -p … --output-format ndjson` run: the conversation, a command and
    /// its result, and the agent's text in two events.
    const A_RECORDED_STREAM: &str = concat!(
        r#"{"type":"system","event_type":"conversation_started","conversation_id":"oz-conv-1"}"#,
        "\n",
        r#"{"type":"tool_call","tool":"run_command","command":"cargo test"}"#,
        "\n",
        r#"{"type":"tool_result","tool":"run_command","status":"complete","exit_code":0,"output":"ok"}"#,
        "\n",
        r#"{"type":"agent","text":"Tests pass. "}"#,
        "\n",
        r#"{"type":"agent","text":"Done."}"#,
        "\n",
    );

    #[test]
    fn the_adapter_reads_a_recorded_oz_stream_into_its_text_its_conversation_and_its_lines() {
        // Given
        let mut conversation_lines = Vec::new();
        let mut on_progress = |_: &ProgressEvent| {};
        let mut on_raw_output = |_: &str| {};
        let mut on_conversation_line = |line: &str| conversation_lines.push(line.to_string());

        // When
        let read = OzNdjson
            .read_stream(
                &mut Cursor::new(A_RECORDED_STREAM.as_bytes()),
                StreamSinks {
                    on_progress: &mut on_progress,
                    on_raw_output: &mut on_raw_output,
                    on_debug_line: None,
                    on_conversation_line: Some(&mut on_conversation_line),
                },
            )
            .expect("the adapter reads the recorded stream");

        // Then
        let lines: Vec<String> = A_RECORDED_STREAM.lines().map(str::to_string).collect();
        assert_eq!(
            (
                read.result_text,
                read.session_id,
                read.raw_lines,
                conversation_lines
            ),
            (
                "Tests pass. Done.".to_string(),
                "oz-conv-1".to_string(),
                lines.clone(),
                lines,
            )
        );
    }

    #[test]
    fn an_oz_exit_is_ok_only_when_zero_even_when_the_spec_allows_structured_results() {
        // Given
        let stream = StreamResult {
            result_text: "Done.".to_string(),
            session_id: "oz-conv-1".to_string(),
            questions: Vec::new(),
            raw_lines: Vec::new(),
            stream_errors: Vec::new(),
        };

        // When
        let ok = (
            OzNdjson.exit_ok(0, &stream, false),
            OzNdjson.exit_ok(1, &stream, true),
        );

        // Then
        assert_eq!(ok, (true, false));
    }

    #[test]
    fn an_oz_stream_is_decoded_by_the_drivers_oz_transcript() {
        // When
        let format = OzNdjson.turn_format();

        // Then
        assert_eq!(format, Some(HeadlessHarness::Oz));
    }
}
