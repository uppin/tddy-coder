//! Whether a workflow's agent can authenticate its GitHub PR tools — the fact a prompt must know
//! before it advertises them.
//!
//! The tools ask the session's host for the project account's token per call
//! (`tddy_tools::github_credential`), and the host can only answer when it was started with a
//! credential handler. A prompt that names the tools to an agent whose host cannot answer promises
//! what every call would refuse, so the recipe's hooks read this flag from the workflow context
//! instead of assuming either answer. It is a context value the driver of the workflow sets, never
//! a probe of the process environment: the environment is not a source of GitHub credentials.

// TODO(keyring 9/9): nothing sets this key in production. The workflows whose hooks read it run in
// a `tddy-coder` process, whose own toolcall listener has no `GithubCredentialHandler`: the
// credential lives in the daemon, and the tool-session spawn path (`tool_session_spawn.rs` ->
// `SpawnRequest` over the supervisor/worker wire) carries no field to hand the child a way to ask
// for one. Until that wire changes (a decision, not an edit), the prompts truthfully stay silent
// about the PR tools. A daemon-managed claude-cli session never runs these hooks: its prompt is the
// recipe's orchestration prompt, and its tools are answered by `SessionGithubCredential`.

use tddy_core::workflow::context::Context;

/// The context key a workflow's driver sets to `true` when the agent's host answers its tools'
/// GitHub token requests.
pub const GITHUB_PR_TOOLS_AVAILABLE_KEY: &str = "github_pr_tools_available";

/// Whether the context says the agent's PR tools can authenticate. `false` when it says nothing.
#[must_use]
pub fn github_pr_tools_available(context: &Context) -> bool {
    context
        .get_sync::<bool>(GITHUB_PR_TOOLS_AVAILABLE_KEY)
        .unwrap_or(false)
}
