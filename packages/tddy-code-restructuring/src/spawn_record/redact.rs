//! What may reach the record file: argv only for programs the policy knows, and no argument
//! that carries a credential.
//!
//! The policy is the JSONL sink's, not the observer trait's: an observer in process sees the raw
//! arguments, and only what is persisted is redacted.

/// The argv a record may carry for `program` run with `args`.
///
/// TODO(spawn-record): implement the policy (allow-listed programs only; URL userinfo, token
/// prefixes, `--*token*`/`--*password*`/`--*secret*`/`--*key*`/`--*auth*` and sensitive `-c`
/// values replaced by `<redacted>`). Until then **every** argument is withheld, so nothing
/// that is written through this function can leak one.
pub fn redacted(program: &str, args: &[String]) -> Vec<String> {
    let _ = program;
    vec![format!(
        "<{} arguments, not recorded: redaction is not implemented>",
        args.len()
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect()
    }

    /// One row per rule the record's policy states: what goes in, and what the file may hold.
    #[test]
    fn redacted_cases() {
        let cases: Vec<(&str, Vec<String>, Vec<String>)> = vec![
            // An allow-listed program keeps its ordinary arguments.
            (
                "cargo",
                argv(&["check", "--all-targets", "-p", "origin"]),
                argv(&["check", "--all-targets", "-p", "origin"]),
            ),
            // URL userinfo is a credential.
            (
                "git",
                argv(&["clone", "https://deploy:hunter2@host/x"]),
                argv(&["clone", "<redacted>"]),
            ),
            // A token by its prefix, whichever flag carries it.
            (
                "git",
                argv(&["push", "ghp_s3cr3tT0ken", "github_pat_s3cr3t", "gho_s3cr3t"]),
                argv(&["push", "<redacted>", "<redacted>", "<redacted>"]),
            ),
            (
                "git",
                argv(&["fetch", "sk-s3cr3t", "xoxb-s3cr3t"]),
                argv(&["fetch", "<redacted>", "<redacted>"]),
            ),
            // `--token=value` carries its own value; `--token value` takes the next argument.
            (
                "git",
                argv(&["push", "--token=s3cr3t"]),
                argv(&["push", "<redacted>"]),
            ),
            (
                "git",
                argv(&["push", "--password", "s3cr3t", "origin"]),
                argv(&["push", "<redacted>", "<redacted>", "origin"]),
            ),
            // A `-c key=value` whose key names a credential loses its value, and keeps its key.
            (
                "git",
                argv(&[
                    "-c",
                    "http.extraheader=Authorization: Bearer s3cr3t",
                    "fetch",
                ]),
                argv(&["-c", "http.extraheader=<redacted>", "fetch"]),
            ),
            (
                "git",
                argv(&["-c", "user.name=somebody", "log"]),
                argv(&["-c", "user.name=somebody", "log"]),
            ),
            // A program off the allow-list has its arguments withheld whole.
            (
                "echo",
                argv(&["alpha", "beta"]),
                argv(&["<2 arguments, not recorded>"]),
            ),
        ];

        for (program, args, expected) in cases {
            assert_eq!(
                redacted(program, &args),
                expected,
                "{program} {args:?} was not redacted as the policy states"
            );
        }
    }
}
