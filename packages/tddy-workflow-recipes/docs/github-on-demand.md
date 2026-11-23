# GitHub access in the workflow recipes

Nothing in this crate reads `GITHUB_TOKEN` or `GH_TOKEN`. The one caller that reaches GitHub with a
token (`merge_pr/github.rs`) takes a plain `token: &str`, refuses a blank one, and has no optional form
that could fall back to anything. A client that asks the session's host for its token instead uses
`tddy_github::pr_api::RealGithubPrApi::asking_the_session_host`, which round-trips
`tddy_core::toolcall::request_github_token_from_session()` ([toolcall `github-token`
verb](../../tddy-toolcall/docs/architecture.md#the-github-token-verb)); a refusal is not remembered and
a token the host handed over is kept for that client's life ([REST
token](../../tddy-github/docs/rest-token.md)).

## Prompt awareness

`merged_red_system_prompt` and the merge-pr system prompts advertise the PR tools only when the
workflow context carries `github_pr_tools_available = true` (`github_pr_tools::github_pr_tools_available`,
`false` when the context says nothing). The key is `tddy_workflow::context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY`,
defined beside the workflow vocabulary so the presenter that seeds it and the hooks that read it name one
string. It is set by the process driving the workflow, never by probing the environment. A
daemon-managed claude-cli session never runs these hooks: its tools are answered by the daemon's
`SessionGithubCredential`.

## Tests

`tests/github_rest_call_carries_the_session_hosts_token.rs` and the structural
`no_crate_still_resolves_a_github_token_from_the_process_environment` in `tddy-daemon-auth`.
