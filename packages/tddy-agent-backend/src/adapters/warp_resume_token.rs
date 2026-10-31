//! `warp-resume-token`: the token of the last `warp --resume <TOKEN>` Warp printed on its terminal
//! as it exited (`#agent-tools 8/12`). Named by `warp.yaml`'s `coding_agent.resume`.

use super::launch::ResumeTokenAdapter;

/// [`WarpResumeToken`]'s name.
pub const WARP_RESUME_TOKEN: &str = "warp-resume-token";

/// Scrapes a Warp session's terminal output, escape sequences and CR line ends included — what
/// `cli_harness_warp::resume_token_from_output` reads today.
#[derive(Debug, Clone, Copy, Default)]
pub struct WarpResumeToken;

impl ResumeTokenAdapter for WarpResumeToken {
    fn name(&self) -> &str {
        WARP_RESUME_TOKEN
    }

    fn resume_token(&self, output: &[u8]) -> Option<String> {
        // TODO(harness-clis): implement — `cli_harness_warp::resume_token_from_output` and its
        // escape-sequence stripping move here when `cli_harness_warp.rs` is deleted.
        let _ = output;
        todo!("harness-clis: WarpResumeToken::resume_token")
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// What Warp prints as it exits: dimmed prose, then the command in bold, CRLF line ends, after
    /// an earlier run's banner scrolled past.
    const RECORDED_EXIT: &[u8] = b"\x1b[?1049l\x1b[2mResume this conversation with:\x1b[0m\r\n  \x1b[1mwarp --resume tok-1\x1b[0m\r\n\x1b]0;warp\x07\x1b[2mResume this conversation with:\x1b[0m\r\n  \x1b[1mwarp --resume 01JB7Q2N5W\x1b[0m\r\n";

    #[test]
    fn the_last_token_a_recorded_exit_printed_is_extracted() {
        // When
        let token = WarpResumeToken.resume_token(RECORDED_EXIT);

        // Then
        assert_eq!(token, Some("01JB7Q2N5W".to_string()));
    }

    #[test]
    fn an_exit_that_printed_no_resume_command_yields_none() {
        // Given — a run that ended before Warp could say how to resume it
        let output = b"\x1b[?1049l\x1b[2mGoodbye\x1b[0m\r\nwarp exited\r\n";

        // When
        let token = WarpResumeToken.resume_token(output);

        // Then
        assert_eq!(token, None);
    }
}
