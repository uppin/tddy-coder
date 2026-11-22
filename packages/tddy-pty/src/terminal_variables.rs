//! The terminal a PTY child is told it runs in. Kept apart from `runtime.rs`, which is over its
//! length budget (`docs/code-issues/oversized-file-runtime.md`).

use portable_pty::CommandBuilder;

/// The variables naming the terminal a PTY child runs in: the terminal this runtime emulates. A
/// process that draws on it at one remove — an agent on a target, behind a transport client — is
/// given the same names.
pub const TERMINAL_VARIABLES: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// Set [`TERMINAL_VARIABLES`] on a PTY child.
pub(crate) fn set_terminal_variables(cmd: &mut CommandBuilder) {
    for (name, value) in TERMINAL_VARIABLES {
        cmd.env(name, value);
    }
}
