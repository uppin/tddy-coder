//! Keys of the values a workflow's driver seeds into its context for the recipes' hooks to read.

/// Set to `true` by the process that drives a workflow when the agent's host answers its tools'
/// GitHub token requests; the recipes' prompts advertise the PR tools only then. A context value the
/// driver sets, never a probe of the process environment: the environment is not a source of GitHub
/// credentials.
pub const GITHUB_PR_TOOLS_AVAILABLE_KEY: &str = "github_pr_tools_available";
