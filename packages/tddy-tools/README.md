# tddy-tools

The binary an agent talks to. Three things in one executable: a CLI of machine-readable subcommands
an agent invokes over `TDDY_SOCKET`, a Claude Code `--permission-prompt-tool` decision engine, and
an `rmcp` MCP server over stdio that routes every tool a session can call.

## Quick Start

### Build
```bash
cargo build -p tddy-tools
```

### Run
```bash
tddy-tools --mcp                       # MCP server over stdio
tddy-tools submit --goal red --data '…'
tddy-tools list-schemas
```

### Test
```bash
cargo test -p tddy-tools
```

## Architecture

`main.rs` parses arguments and dispatches; `server.rs` is the MCP server. Almost nothing else is
implemented here. Each subcommand's logic lives in the crate that owns its domain —
`tddy_bsp::build_cli`, `tddy_code_analysis::analyze_cli`,
`tddy_code_restructuring::restructure_cli`, `tddy_workflow_recipes::schema`,
`tddy_core::toolcall::client` — and each MCP tool's implementation likewise, in
`tddy_tool_engine`, `tddy_lsp_executor::lsp_tools`, `tddy_workflow_recipes::pr_stack` and
`tddy_discovery::{roster, subagent_runtime}`. What this crate contributes is the *shape* those
things take on the wire: the clap surface, the `#[tool]` advertisement, and the router that assembles
them into one server.

## The rule that decides what lives here

**The shape of an interface belongs to the crate that speaks that interface.** MCP shapes stay where
`rmcp` is; clap surfaces stay where arguments are parsed; stdout stays in a binary, never in a
library the TUI links. So the permission-decision engine, the `ServerHandler` router, the 14 `pr_*`
MCP tools' advertisement, the dynamic tool list, and `PtyRelayArgs`' twenty flags are all here — while
the PR-stack functions behind those tools, the relay behind those flags, and the exec-tool catalog
behind that list are not.

The consequence is a small crate with a wide reach: six public modules, twelve `src` files,
28 dependencies. `session_tool_client` and `session_agents` are re-exports of
`tddy_session_tool_client` and `tddy_discovery::roster`, kept at the paths they were reached by when
they lived here.

## The MCP surface

A session advertises **46** tools when its host claims it serves the session-action surface
(`TDDY_SESSION_ACTION_TOOLS`), and **43** when it does not — `request_action`, `list_actions` and
`invoke_action` are the difference. A transport alone cannot answer that question: the in-jail socket
serves both a handler that implements all three and one that implements none, and the server cannot
tell them apart from the socket. Only the host knows, so only the host says.

`tests/mcp_tool_advertisement_audit.rs` pins both sets **by name** over the real `--mcp` stdio wire,
and pins the difference as a difference. A tool added to both paths keeps it green; one added to only
the claiming path fails.

## Conversation worktrees

With Managed access each conversation's subagent loop is built with its own dispatch closure, which
names the conversation: the daemon runs its calls in the conversation's worktree and each mutating
call's result carries `worktreeChange`. `subagent_end { sessionId }` (`src/subagent_end.rs`) refuses
while a turn is outstanding, pulls the conversation's work into the caller's worktree as uncommitted
changes, closes the conversation as `subagent_cancel` does, and answers `{ended, pulled}`;
`subagent_cancel` removes the worktree without pulling. A conversation whose loop runs on the daemon
has no conversation worktree, so there is nothing to pull or remove.

`subagent_resume { fromMessageId }` also takes the conversation's worktree back to the rewind point.
The `resetWorktree` boolean (default `true`; `false` rewinds the transcript only; a non-boolean is
refused by name) is read by `reset_worktree_choice` (`src/reset_worktree_choice.rs`, its own module
because `server.rs` is over the file budget). Each Managed conversation is built with a
`ConversationWorktreeResetPort` (`src/worktree_reset_port.rs`) beside its dispatch closure; it sends
`ConversationWorktree { reset }` through `tddy_session_tool_client::reset_conversation_worktree` and
maps the answer — `{"reset": {to, droppedCommits}}`, `{"reset": null}` for no worktree, or an error
body that refuses the resume. The turn outcome then carries `worktreeReset`.

`subagent_diff { sessionId, from?, to? }` (`src/subagent_diff.rs`) is read-only and takes no lock on
the conversation, so it answers while a turn runs. It sends `ConversationWorktree { diff }` through
`tddy_session_tool_client::diff_conversation_worktree` and answers the daemon's `diff` object —
`{from, to, files, lines, diff, truncated}`, the diff text capped at 64 KiB while the counts cover the
whole range. An unknown conversation, one that never wrote, and a commit the conversation does not
have (a dropped one included) are refused by name.

`subagent_pull { sessionId, from?, to? }` (`src/subagent_pull.rs`) takes a range of the conversation's
commits into the caller's worktree while the conversation carries on; `subagent_end` takes the same
`from` / `to`. Both bounds are **inclusive** (unlike `subagent_diff`'s `from..to`), `from` defaults to
the earliest commit not yet pulled and `to` to the tip, and both send `ConversationWorktree { pull_range }`
through `tddy_session_tool_client::pull_conversation_range`, answering `{pulled: {commits, skipped, files,
lines, conflicts} | null}`. Each is refused while a turn is outstanding. The commits a conversation has
pulled are its **ledger** (`PullLedger`, `src/pull_ledger.rs`): every pull carries it so the daemon skips
what was taken, it is not persisted, and it is dropped when the conversation closes. When a rewind drops
commits that were pulled, the turn outcome's `worktreeReset` gains `droppedPulledCommits` (what was
pulled stays in the caller's worktree; nothing is un-applied).

## `restructure`

`restructure` is `tddy_code_restructuring::restructure_cli` for the command line, or the
`code_index.CodeIndexService` of a running `tddy-index-daemon` when **`TDDY_INDEX_SOCKET`** is set
(`index_client.rs`, rendered by `index_console.rs`). `load`, `unload` and `plans` need the daemon;
`snapshot` stays in process, so a `snapshot` of an item-anchored plan starts its own language server
even with a daemon running. `--items` is read by `tddy_code_restructuring::item_anchor::parse_item_list`,
the same rule as the other front ends. The stale operations a daemon reports on `ListPlans`,
`PlanStatus` and `Check` are printed by `console::stale_operations`, the renderer the in-process CLI
uses, so the two paths read alike; `Apply` refuses a stale operation (`FailedPrecondition`) before
writing anything. See [Rust code restructuring](../../docs/ft/coder/rust-code-restructuring.md#live-plans).

## The environment is the real interface

Twenty-one `TDDY_*` variables, `TDDY_SOCKET` read at 43 sites, are how an in-jail agent reaches its
host and how a host declares what it serves. They are not renamed — an agent inside a jail has no
other way in — and several are now read from other crates under their original spellings.
`TDDY_TOOLS_LOG_FILE` redirects `env_logger` to a file (append), so an in-jail server's logs land
where the host can read them instead of vanishing into a captured stderr.

## Testing

46 test files, most of them driving the built binary from outside — `assert_cmd` over the real
`--mcp` stdio wire, or the real CLI. That is deliberate: those suites prove the surface regardless of
which crate now implements it, which is why they stayed here when their implementations moved.

## Documentation

### Technical implementation (how)
- [Changesets](./docs/changesets/) — applied changeset history
- [JSON Schema embedding and validation](../tddy-workflow-recipes/docs/json-schema.md) — the schema library behind `submit` / `get-schema` / `list-schemas`

### Product requirements (what)
- [Session actions](../../docs/ft/coder/session-actions.md) — `list-actions`, `invoke-action`, `request_action`
- [GitHub pull request tools (MCP)](../../docs/ft/coder/github-pr-tools-mcp.md)
- [Sandboxed codebase mode](../../docs/ft/coder/sandboxed-codebase-mode.md) — the jail this runs inside
- [Session agent roster](../../docs/ft/daemon/session-agent-roster.md) — the roster this server follows

## Related packages
- [`tddy-session-tool-client`](../tddy-session-tool-client/README.md) — how a tool call reaches this session's daemon
- [`tddy-discovery`](../tddy-discovery/docs/roster-and-subagent-runtime.md) — the live agent roster and the subagent conversation runtime
- [`tddy-tool-engine`](../tddy-tool-engine/README.md) — the exec-tool catalog and its execution
- [`tddy-workflow-recipes`](../tddy-workflow-recipes/docs/workflow-schemas.md) — `goals.json`, the schemas, and the PR-stack functions
- [`tddy-toolcall`](../tddy-toolcall/README.md), [`tddy-session-actions`](../tddy-session-actions/README.md), [`tddy-agent-backend`](../tddy-agent-backend/README.md) — the toolcall wire, session actions, and the model catalogue (all re-exported by `tddy-core`)
- [`tddy-terminal-rpc`](../tddy-terminal-rpc/) — the PTY relay behind `pty-relay`
