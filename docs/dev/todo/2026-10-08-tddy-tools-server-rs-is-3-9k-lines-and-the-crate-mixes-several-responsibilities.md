# 2026-10-08 — `tddy-tools`: `server.rs` is 3.9k lines and the crate mixes several responsibilities

**Category:** Future enhancement (file and crate responsibility)
**Source:** #carve 21/21 (PR #536) — LoC survey of `packages/`, 2026-10-08. A TODO only: no plan yet.

What is known (raw lines, inline tests included; measured 2026-10-08):

- `tddy-tools` is ~9.1k lines in `src/` (25 files), 23.6k with `tests/` (14.5k of it tests). Two
  binaries: `tddy-tools` and the `execute-tool-stdio-fixture` test fixture. 28 workspace dependencies.
- **`src/server.rs` alone is 3,902 lines — 43 % of the crate** and about eight times the repo's
  500-line file limit. Next largest: `cli.rs` 910, `index_console.rs` 557, `index_client.rs` 524,
  `remote_cli.rs` 357, `mcp_primitives.rs` 355, `session_hook.rs` 328, `pull_ledger.rs` 244,
  `pty_relay.rs` 222, `main.rs` 211, `action_tools.rs` 202.
- The crate doc (`lib.rs`) describes it as the MCP server plus the CLI's supporting surface, and
  records that the roster and subagent conversation runtime already left it for `tddy_discovery`
  (`#unbundle` node 5): "this crate advertises those tools and no longer implements them".
- By file name, and **not verified by reading**, it appears to hold: the MCP server (`server`,
  `mcp_primitives`, `tool_list_announcer`), the CLI relay (`cli`, `remote_cli`, `session_hook`,
  `pty_relay`), the warm-index client and console (`index_client`, `index_console`,
  `restructure_tools`), subagent worktree sync/reset/pull/diff ports and choices (`subagent_*`,
  `worktree_*`, `*_worktree_choice`, `pull_ledger`), and small pieces (`github_credential`,
  `list_models`, `action_tools`, `session_actions_cli`).

## Open

- What `server.rs` is made of, and which parts are separable (not yet read).
- Whether the subagent worktree ports and the index client belong in `tddy-tools` at all, or with the
  crates that own those domains (`tddy-subagent-worktree`, `tddy-index-daemon`, a future subagent crate
  from [the `tddy-discovery` drain](2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md)).
- The 500-line limit applies to `server.rs` and `cli.rs` regardless of any crate split.

## Why deferred

Found while surveying sizes during #536, a move node on a different crate. Needs reading `server.rs`
before any split is proposed.
