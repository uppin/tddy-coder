//! What may reach the record file: argv only for programs the policy knows, and no argument
//! that carries a credential.
//!
//! The policy is the JSONL sink's, not the observer trait's: an observer in process sees the raw
//! arguments, and only what is persisted is redacted.

use std::path::Path;

/// The placeholder a withheld argument is replaced by.
const REDACTED: &str = "<redacted>";

/// The programs whose ordinary arguments the record may carry.
const ALLOW_LIST: [&str; 7] = [
    "git",
    "cargo",
    "rustfmt",
    "rust-analyzer",
    "nix",
    "setsid",
    "sh",
];

/// The argument prefixes that mark a bare value as a token.
const TOKEN_PREFIXES: [&str; 5] = ["ghp_", "gho_", "github_pat_", "sk-", "xox"];

/// The flag-name fragments that mark the value they carry as a credential.
const SENSITIVE_FLAGS: [&str; 5] = ["token", "password", "secret", "key", "auth"];

/// The config-key fragments that mark a `-c key=value` value as a credential.
const SENSITIVE_KEYS: [&str; 5] = ["token", "auth", "header", "password", "secret"];

/// The argv a record may carry for `program` run with `args`.
///
/// A program off the allow-list keeps none of its arguments — only a count of them. For an
/// allow-listed program, an argument is replaced by `<redacted>` when it carries URL userinfo,
/// looks like a token, is the value of a `--*token*`-shaped flag (in either `--flag=value` or
/// `--flag value` form), or is the value of a `-c <key>=<value>` whose key names a credential —
/// where the key is kept, so `http.extraheader=<redacted>` still says which header was set.
pub fn redacted(program: &str, args: &[String]) -> Vec<String> {
    if !is_allow_listed(program) {
        return vec![format!("<{} arguments, not recorded>", args.len())];
    }

    let mut kept = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];

        // `-c key=value`: the key says which setting, the value may be a credential.
        if argument == "-c" {
            kept.push("-c".to_string());
            if let Some(assignment) = args.get(index + 1) {
                kept.push(redact_config_value(assignment));
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }

        // `--flag=value`: a sensitive flag takes its own value with it.
        if let Some((flag, _value)) = argument.split_once('=') {
            if is_sensitive_flag(flag) {
                kept.push(REDACTED.to_string());
                index += 1;
                continue;
            }
        }

        // `--flag value`: a sensitive flag withholds itself and the argument after it.
        if is_sensitive_flag(argument) {
            kept.push(REDACTED.to_string());
            if let Some(_value) = args.get(index + 1) {
                kept.push(REDACTED.to_string());
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }

        if carries_a_credential(argument) {
            kept.push(REDACTED.to_string());
        } else {
            kept.push(argument.clone());
        }
        index += 1;
    }
    kept
}

/// Whether the policy knows `program`, by its name rather than the path it was named by.
fn is_allow_listed(program: &str) -> bool {
    let name = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    ALLOW_LIST.contains(&name)
}

/// Whether `argument` on its own carries a credential: URL userinfo or a token prefix.
fn carries_a_credential(argument: &str) -> bool {
    has_url_userinfo(argument)
        || TOKEN_PREFIXES
            .iter()
            .any(|prefix| argument.starts_with(prefix))
}

/// Whether `argument` is a URL with `user:password@` in it.
fn has_url_userinfo(argument: &str) -> bool {
    let Some((_scheme, rest)) = argument.split_once("://") else {
        return false;
    };
    let Some((userinfo, _host)) = rest.split_once('@') else {
        return false;
    };
    userinfo.contains(':')
}

/// Whether `flag` is a `--`-flag whose name names a credential.
fn is_sensitive_flag(flag: &str) -> bool {
    let Some(name) = flag.strip_prefix("--") else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    SENSITIVE_FLAGS
        .iter()
        .any(|fragment| name.contains(fragment))
}

/// A `-c key=value` assignment with a credential's value replaced, keeping the key.
fn redact_config_value(assignment: &str) -> String {
    let Some((key, _value)) = assignment.split_once('=') else {
        return assignment.to_string();
    };
    if key_names_a_credential(key) {
        format!("{key}={REDACTED}")
    } else {
        assignment.to_string()
    }
}

/// Whether a config key names a credential.
fn key_names_a_credential(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|fragment| key.contains(fragment))
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
