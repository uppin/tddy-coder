# GitHub access in the PR-stack workflow

The engine-driven PR-stack tasks (`AssessTask`, `RepointTask`, `MergeTask` of
`OrchestratePrStackRecipe`) reach GitHub with a token **asked of the session's host**, and only when an
action that reaches GitHub runs. Nothing in this crate reads `GITHUB_TOKEN` or `GH_TOKEN`; the
caller that did (`merge_pr/github.rs`) takes a plain `token: &str`, refuses a blank one, and has no
optional form that could fall back to anything.

The recipe is retained but inert in production (`recipe_resolve` maps every CLI name to
`PrStackRecipe`), so these tasks have no live driver. The live callers of the same helpers
(`assemble_views`, `execute_stack_merge`, `execute_stack_repoint`) are the `pr_*` tools, which already
pass a token.

## Asking

The tasks ask with the round trip `tddy-tools` makes:
`tddy_core::toolcall::request_github_token_from_session()`, reading `TDDY_SOCKET` only to learn whom to
ask ([toolcall `github-token` verb](../../tddy-toolcall/docs/architecture.md#the-github-token-verb)).

| Task | When it asks |
|---|---|
| construction (`build_graph`, `*Task::new()`) | never |
| `AssessTask` | `RealGithubPrApi::asking_the_session_host`: the first time a node that owns a branch has its PR looked up. A stack whose nodes own none asks nothing. A refusal propagates through `assemble_views`'s `?` |
| `RepointTask` | up front, only when `repoint_reaches_github` — some dependent owns a branch, so there is a PR to re-target. Up front rather than lazily, because `execute_stack_repoint` downgrades a failed GitHub call to a `warn`, and a lazily surfaced refusal would be swallowed while the dependents' PRs kept their old base |
| `MergeTask` | at the top of `run`: merging always reaches GitHub, and asking *before* `execute_stack_merge` keeps a refusal from leaving a `Planned` journal behind (no `stack-op.json` after a refused merge) |

`RealGithubPrApi::asking` holds a supplier, not a token: a refusal is not remembered and a token the
host handed over is kept for that client's life ([REST token](../../tddy-github/docs/rest-token.md)).

## Prompt awareness

`merged_red_system_prompt` and the merge-pr system prompts advertise the PR tools only when the
workflow context carries `github_pr_tools_available = true` (`github_pr_tools::github_pr_tools_available`,
`false` when the context says nothing). The key is `tddy_workflow::context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY`,
defined beside the workflow vocabulary so the presenter that seeds it and the hooks that read it name one
string. It is set by the process driving the workflow, never by probing the environment. A
daemon-managed claude-cli session never runs these hooks: its tools are answered by the daemon's
`SessionGithubCredential`.

## Tests

`tests/orchestrate_pr_stack_github_on_demand.rs` (against a real socket host that counts the requests it
receives), `tests/github_rest_call_carries_the_session_hosts_token.rs`, and the structural
`no_crate_still_resolves_a_github_token_from_the_process_environment` in `tddy-daemon-auth`.
