//! `warp-resume-token`: the token of the last `warp --resume <TOKEN>` Warp printed on its terminal
//! as it exited (`#agent-tools 8/12`). Named by `warp.yaml`'s `coding_agent.resume`.

use super::launch::ResumeTokenAdapter;

/// [`WarpResumeToken`]'s name.
pub const WARP_RESUME_TOKEN: &str = "warp-resume-token";

/// Scrapes a Warp session's terminal output, escape sequences and CR line ends included: the token
/// of the last `warp --resume <TOKEN>` line the output holds, `None` when it holds none.
#[derive(Debug, Clone, Copy, Default)]
pub struct WarpResumeToken;

impl ResumeTokenAdapter for WarpResumeToken {
    fn name(&self) -> &str {
        WARP_RESUME_TOKEN
    }

    fn resume_token(&self, output: &[u8]) -> Option<String> {
        let plain = String::from_utf8_lossy(&without_escape_sequences(output)).replace('\r', "\n");
        let mut token = None;
        for line in plain.lines() {
            let words: Vec<&str> = line.split_whitespace().collect();
            for window in words.windows(3) {
                if let ["warp", "--resume", found] = window {
                    token = Some(found.to_string());
                }
            }
        }
        token
    }
}

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// `output` without its escape sequences: CSI (`ESC [` … a final byte), the string sequences OSC,
/// DCS, SOS, PM and APC (closed by BEL or `ESC \`), and any other escape (its intermediate bytes,
/// then one final byte).
fn without_escape_sequences(output: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(output.len());
    let mut bytes = output.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte != ESC {
            plain.push(byte);
            continue;
        }
        match bytes.next() {
            Some(b'[') => {
                bytes.find(|b| (0x40..=0x7e).contains(b));
            }
            Some(b']' | b'P' | b'X' | b'^' | b'_') => skip_string(&mut bytes),
            Some(0x20..=0x2f) => {
                bytes.find(|b| (0x30..=0x7e).contains(b));
            }
            Some(_) | None => {}
        }
    }
    plain
}

/// Consumes a string sequence up to and including its terminator: BEL, or `ESC \`.
fn skip_string(bytes: &mut impl Iterator<Item = u8>) {
    let mut after_esc = false;
    for byte in bytes {
        if byte == BEL || (after_esc && byte == b'\\') {
            return;
        }
        after_esc = byte == ESC;
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
