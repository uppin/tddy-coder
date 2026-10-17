//! The helper's one request to the daemon's loopback askpass endpoint.

use std::fmt;
use std::time::Duration;

use prost::Message;
use tddy_service::proto::askpass::{AskpassRequest, AskpassResponse, Outcome, PROMPT_PATH};

/// How long the helper waits for the daemon's reply.
///
/// The daemon holds the request open while a person types, for at most the prompt's lifetime
/// (120 s) and then answers "expired". The client must outlast that, so a slow answer is never
/// mistaken for an unreachable daemon: the lifetime plus a minute of headroom.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(180);

/// Why no answer was obtained. Never carries the answer itself.
#[derive(Debug, PartialEq, Eq)]
pub enum AskpassError {
    /// The endpoint did not answer.
    Unreachable { url: String, reason: String },
    /// The daemon declined to relay the request.
    Refused(String),
    /// Nobody answered in time.
    Expired,
    /// The endpoint answered with something that is not an askpass reply.
    Malformed(String),
}

impl fmt::Display for AskpassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { url, reason } => {
                write!(
                    f,
                    "the daemon's askpass endpoint at {url} is unreachable: {reason}"
                )
            }
            Self::Refused(reason) => write!(f, "the daemon refused the request: {reason}"),
            Self::Expired => write!(f, "the passphrase prompt expired unanswered"),
            Self::Malformed(why) => write!(f, "the daemon sent a malformed reply: {why}"),
        }
    }
}

/// What the operator answered, and whether the daemon saw this clone answered for the same key
/// before — which, when ssh asks again, means the earlier answer was wrong.
pub struct Answer {
    pub passphrase: Vec<u8>,
    pub previous_answer_rejected: bool,
}

/// Ask the daemon at `url`, as the clone holding `token`, for the answer to `prompt`.
pub fn ask(url: &str, token: &str, prompt: &str) -> Result<Answer, AskpassError> {
    let unreachable = |e: reqwest::Error| AskpassError::Unreachable {
        url: url.to_string(),
        // Only the error's own text: the request body holds no secret, but the reply may.
        reason: e.without_url().to_string(),
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(REPLY_TIMEOUT)
        .build()
        .map_err(unreachable)?;
    let body = AskpassRequest {
        token: token.to_string(),
        prompt: prompt.to_string(),
    }
    .encode_to_vec();
    let reply = client
        .post(format!("{}{PROMPT_PATH}", url.trim_end_matches('/')))
        .header("content-type", "application/proto")
        .body(body)
        .send()
        .map_err(unreachable)?;
    if !reply.status().is_success() {
        return Err(AskpassError::Malformed(format!("HTTP {}", reply.status())));
    }
    let bytes = reply.bytes().map_err(unreachable)?;
    let response =
        AskpassResponse::decode(bytes).map_err(|e| AskpassError::Malformed(e.to_string()))?;
    match Outcome::try_from(response.outcome) {
        Ok(Outcome::Answer) => Ok(Answer {
            passphrase: response.answer,
            previous_answer_rejected: response.previous_answer_rejected,
        }),
        Ok(Outcome::Refused) => Err(AskpassError::Refused(response.reason)),
        Ok(Outcome::Expired) => Err(AskpassError::Expired),
        _ => Err(AskpassError::Malformed(format!(
            "unknown outcome {}",
            response.outcome
        ))),
    }
}
