# 2026-11-22 — The pr-stack workflow recipe is removed; stack helpers live in `tddy-pr-stack`

**Type:** Feature (removal) — `#workqueue` 1/14, PR #1002

The `pr-stack` workflow recipe, its aliases (`plan-pr-stack`, `orchestrate-pr-stack`), its goals
(`analyze-stack`, `write-stack-plan`, `write-stack-docs`, `orchestrate`) and their schemas,
`PrStackScreen` and its private modules, stack base-session seeding end to end
(`--stack-seed-base-session`, `StartSessionRequest` field 31 — now reserved — the MCP `StartSession`
argument, the daemon validation, the spawn plumbing and the web picker), the form's stack parent
picker, and the `spawn-child` seam (`StackChildSpawnHandler`, the `ChildSpawnHandler` trait, the
`SpawnChild` wire arm and its plumbing) are deleted. Stacks are planned and driven through the
project stack view and the `pr_*` tools. A session recorded with the removed recipe lists and opens
in the workflow chat view; its resume is refused with the recipe named.

The stack helpers the recipe module still held moved into `tddy-pr-stack` verbatim —
`internal_status`, `op_journal` (`StackOpJournal`, `MergePhase`, `recover_in_flight_stack_op`),
`merge_repoint` (`execute_stack_merge`, `execute_stack_repoint`) and `PR_STACK_PLAN_MD_BASENAME` —
and every consumer names `tddy_pr_stack` / `tddy_github` directly. The GitHub client's three
acceptance suites moved to `tddy-github`.

Also in this PR:

- **Coder one-shot watchdog** (`tddy-remote-shell`): the wrapper's watchdog closes fd 3 (the
  command's stderr). A `TERM` that missed the watchdog's `sleep` left stderr held until the deadline,
  which is how a 60 s `kill -0` liveness probe answered `AgentNeverAnswered` intermittently in
  `coder_agent_launch_acceptance::a_hook_fired_in_a_coder_workspace_reaches_the_daemon`. Debug-level
  timings now cover each liveness probe and each one-shot's client exit and pipe ends.
- **Test isolation**: `tddy-tools` `clone_placement_exec_dispatch_acceptance` removes every session
  transport that outranks the HTTP relay (it inherited `TDDY_REMOTE_DAEMON_SOCKET` inside a session);
  `tddy-tools-cli` `notify_cli_acceptance` runs from the temp dir, out of reach of a worktree's
  `.tddy/session.json`.

Backlog entries resolved and deleted: *Retire `StackChildSpawnHandler` and the `spawn-child`
toolcall plumbing* (2026-11-14-retire-stack-child-spawn-handler); *`stack-progress.json` is
documented as a host guarantee but nothing writes it* (2026-08-29-stack-progress-json-…, the
contract's doc section is deleted); *pr-stack — a managed claude-cli/cursor-cli session may record a
goal id as its state* (2026-08-13-pr-stack-a-managed-…); *pr-stack — an externally-located worktree
is refused as a stack base* (2026-08-13-pr-stack-an-externally-located-…); *pr-stack — seeding a stack
from several existing sessions* (2026-08-13-pr-stack-seeding-…). Narrowed: *The New-session form
offers 8 of the daemon's 10 workflow recipes* (both now list the same 7; the drift hazard remains).
Added: *Stack helpers with no production caller after the pr-stack recipe's removal*.

Code issues closed, with their final measurements:

- `tddy-workflow-recipes` **squatting-pr-stack-data-model** — 0 references to
  `tddy_workflow_recipes::{pr_stack,orchestrate_pr_stack,plan_pr_stack}` outside the crate (45 in 4
  crates at 2026-09-22), 0 stack modules left in the crate; `tddy-workflow-recipes` depends on
  `tddy-pr-stack` only for its tests.
- `tddy-workflow-recipes` **misplaced-tests-github-rest-client** — the 3 suites (283 lines) live in
  `tddy-github/tests/` and import `tddy_github` only.

Moved: **complexity-bridge-execute-stack-repoint** → `tddy-pr-stack` as
`complexity-merge-repoint-execute-stack-repoint` (120 lines, nesting 8, unchanged). Re-measured:
`tddy-coder` `run.rs` 2482 → 2359, `tddy-daemon-rpc` `pr_stack/ports.rs` 880 → 876, `tddy-toolcall`
`listener.rs` 881 → 833 production lines (all still over budget; records kept).

Feature docs: [pr-stacking.md](../../ft/coder/pr-stacking.md),
[pr-stack-docs.md](../../ft/coder/pr-stack-docs.md),
[pr-stack-live-status.md](../../ft/coder/pr-stack-live-status.md),
[workflow-recipes.md](../../ft/coder/workflow-recipes.md),
[session-drawer.md § Project stack view](../../ft/web/session-drawer.md#project-stack-view).
