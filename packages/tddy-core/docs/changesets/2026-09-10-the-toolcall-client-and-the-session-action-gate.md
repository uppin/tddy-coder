# 2026-09-10 — The toolcall client, the session-action gate and one reading of a blank variable

**Type:** Architecture

Node 5 of the `#unbundle` stack ([#474](https://github.com/uppin/tddy-coder/pull/474)). Full story in
the cross-package entry:
[2026-09-10-unbundle-tools-thinning.md](../../../../docs/dev/changesets/2026-09-10-unbundle-tools-thinning.md).

Gained from `tddy-tools`, and recorded in [architecture.md](../architecture.md):

- **`toolcall::client::dispatch_toolcall`** — the client half of a wire whose listener this crate
  already hosted. Both ends of one framing are now defined together, so a change cannot be made to
  one and not the other. `toolcall::client_wire` carries the CLI's request/response shapes beside
  their `*RequestWire` counterparts, and `AskQuestionItem` re-exports `backend::QuestionOption`
  rather than declaring a second copy of it.
- **`session_actions::authoring`** — the rules a subagent-authored manifest must satisfy, shared by
  the crate that advertises `request_action` and the one that establishes it.
- **`session_actions::session_dir`** and **`backend::model_catalog`** — the logic halves of two CLI
  subcommands whose surfaces stayed behind.
- **`session_actions::tool_gate`** — `SESSION_ACTION_TOOLS_ENV` and `session_action_tools_enabled`,
  the host's claim that it serves the three session-action tools.
- **`spawn_env::env_non_empty`** — one reading of "unset or blank" for the whole workspace.
- **`session_context`** — moved verbatim, 66 lines.

**`anyhow` was added, on an explicit developer decision.** The crate was deliberately
`thiserror`-only, and three movers were blocked on that alone. The alternative was to rewrite each
moved body into this crate's error style — rejected, because it breaks the verbatim-move property the
node's behaviour-preserving claim rests on, turning a restructure into a rewrite of four modules'
error handling.

**Two movers were split rather than moved, and `clap` stayed out.** `list_models` (172 → 52) and
`session_actions_cli` (136 → 66) both parse arguments and `println!` a JSON contract, and CLAUDE.md
forbids direct stdout in any path that runs under the TUI — which links this crate. The logic came
down; the surface and the stdout stayed in the binary.

**Collapsing `env_non_empty` settled a disagreement rather than picking a side.** Of the three
spellings of "unset or blank" in the workspace, one trimmed before testing for empty and one did not,
so a LiveKit variable exported as `" "` used to configure a transport whose URL was one space. It now
reads as unset, which is what the surviving copy's doc already argued for — the one behaviour
difference in the move. `tddy_daemon_kernel::config::non_empty_env` is a third copy left standing and
[recorded](../../../../docs/dev/todo/2026-09-10-two-duplications-left-standing-by-the-tools-thinning.md),
with `MAX_MANIFEST_BYTES`, which is `64 * 1024` here and in `tddy_sandbox_app::host_actions`.

Log targets moved with their code: `tddy_core::session_context` (3 sites) and
`tddy_core::session_actions::session_dir` (2, of the four that module had — the other two stayed in
`tddy-tools` with the surface). `RUST_LOG=tddy_tools=debug` no longer shows session-context or
session-directory resolution; `RUST_LOG=tddy_core=debug` does.

Tests: 589 / 0.
