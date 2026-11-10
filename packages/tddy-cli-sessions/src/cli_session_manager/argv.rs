//! How an already-rendered argv gains its launch flags.
//!
//! The argv itself is rendered from the agent's YAML spec by the caller that owns the spec
//! (`CodingAgentSpec::render_run_argv`); this module only splices the flags a spawn carries
//! (a system-prompt file, a spawn's extra args) into it, generically for any agent, and the
//! sandbox-mode binary remapping in [`wrap_binary_for_sandbox`](super::wrap_binary_for_sandbox).

use std::path::Path;

impl super::CliSessionManager {
    /// `argv` with the flags a spawn adds: `--append-system-prompt-file <path>` when
    /// `append_system_prompt_file` is given, then `extra_args` (a split session's tool
    /// allowlist, `--mcp-config <path>`), each inserted before a trailing positional prompt
    /// when the argv carries one (`initial_prompt`), so the flags precede the prompt.
    pub fn with_launch_flags(
        argv: Vec<String>,
        initial_prompt: Option<&str>,
        append_system_prompt_file: Option<&Path>,
        extra_args: &[String],
    ) -> Vec<String> {
        let mut with_flags = extra_args.to_vec();
        if let Some(path) = append_system_prompt_file {
            with_flags.splice(
                0..0,
                [
                    "--append-system-prompt-file".to_string(),
                    path.display().to_string(),
                ],
            );
        }
        if with_flags.is_empty() {
            return argv;
        }
        let prompt_present = initial_prompt.is_some_and(|p| !p.trim().is_empty());
        if prompt_present && !argv.is_empty() {
            let mut argv = argv;
            let prompt = argv.pop().expect("checked non-empty");
            argv.extend(with_flags);
            argv.push(prompt);
            argv
        } else {
            let mut argv = argv;
            argv.extend(with_flags);
            argv
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli_session_manager::CliSessionManager;
    use std::path::Path;

    #[test]
    fn extra_args_are_inserted_before_a_trailing_positional_prompt() {
        let argv = CliSessionManager::with_launch_flags(
            vec![
                "/bin/agent".into(),
                "--model".into(),
                "m".into(),
                "do it".into(),
            ],
            Some("do it"),
            None,
            &["--allowedTools".into(), "Read".into()],
        );
        assert_eq!(
            argv,
            vec![
                "/bin/agent".to_string(),
                "--model".to_string(),
                "m".to_string(),
                "--allowedTools".to_string(),
                "Read".to_string(),
                "do it".to_string(),
            ],
            "flags must precede the prompt"
        );
    }

    #[test]
    fn extra_args_append_at_the_end_when_there_is_no_prompt() {
        let argv = CliSessionManager::with_launch_flags(
            vec!["/bin/agent".into()],
            None,
            Some(Path::new("/tmp/prompt.md")),
            &[],
        );
        assert_eq!(
            argv,
            vec![
                "/bin/agent".to_string(),
                "--append-system-prompt-file".to_string(),
                "/tmp/prompt.md".to_string(),
            ]
        );
    }
}
