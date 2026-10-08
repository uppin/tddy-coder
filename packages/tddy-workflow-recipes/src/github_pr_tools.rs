//! Whether a workflow's agent can authenticate its GitHub PR tools — the fact a prompt must know
//! before it advertises them.
//!
//! The tools ask the session's host for the project account's token per call
//! (`tddy_tools::github_credential`), and the host can only answer when it was started with a
//! credential handler. A prompt that names the tools to an agent whose host cannot answer promises
//! what every call would refuse, so the recipe's hooks read this flag from the workflow context
//! instead of assuming either answer. It is a context value the driver of the workflow sets, never
//! a probe of the process environment: the environment is not a source of GitHub credentials.

// The driver that sets this key is the `tddy-coder` process: it seeds `true` exactly when it binds a
// `GithubCredentialHandler` on its own toolcall listener — i.e. when the daemon gave it a
// `--host-session-socket` and the session's id to name itself with (see `tddy-coder`'s
// `tool_host_wiring`). A daemon-managed claude-cli session never runs these hooks: its prompt is
// the recipe's orchestration prompt, and its tools are answered by `SessionGithubCredential`.

use tddy_core::workflow::context::Context;

/// The context key a workflow's driver sets to `true` when the agent's host answers its tools'
/// GitHub token requests. Defined beside the workflow vocabulary so the presenter that seeds it and
/// the hooks that read it name one string.
pub use tddy_workflow::context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY;

/// Whether the context says the agent's PR tools can authenticate. `false` when it says nothing.
#[must_use]
pub fn github_pr_tools_available(context: &Context) -> bool {
    context
        .get_sync::<bool>(GITHUB_PR_TOOLS_AVAILABLE_KEY)
        .unwrap_or(false)
}
