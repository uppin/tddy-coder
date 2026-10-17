//! `tddy-askpass`: ssh runs this as `SSH_ASKPASS` with its question as the only argument.
//!
//! The answer goes to stdout for ssh and nowhere else; every diagnostic goes to stderr and none
//! contains the answer.

use std::io::Write;
use std::process::ExitCode;

use tddy_askpass::client;
use tddy_askpass::prompt_text::{classify, AskpassPrompt};

fn required(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("tddy-askpass: {name} is not set"))
}

fn run() -> Result<Vec<u8>, String> {
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let url = required("TDDY_ASKPASS_URL")?;
    let token = required("TDDY_ASKPASS_TOKEN")?;
    if classify(&prompt) == AskpassPrompt::Other {
        return Err("tddy-askpass: not a key passphrase request, refusing to relay it".into());
    }
    let answer = client::ask(&url, &token, &prompt).map_err(|e| format!("tddy-askpass: {e}"))?;
    if answer.previous_answer_rejected {
        // Only the key's path: the earlier passphrase is not known here, and would not be printed.
        // ssh passes this stderr through, so it ends up in the text of the clone's failure.
        if let AskpassPrompt::KeyPassphrase { key } = classify(&prompt) {
            eprintln!(
                "tddy-askpass: the previous passphrase for key '{key}' was rejected (incorrect passphrase)"
            );
        }
    }
    Ok(answer.passphrase)
}

fn main() -> ExitCode {
    match run() {
        Ok(mut answer) => {
            answer.push(b'\n');
            let mut out = std::io::stdout().lock();
            if out.write_all(&answer).and_then(|()| out.flush()).is_err() {
                eprintln!("tddy-askpass: could not write the answer to stdout");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
