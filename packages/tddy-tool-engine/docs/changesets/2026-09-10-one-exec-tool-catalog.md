# 2026-09-10 — One exec-tool catalog, and the remote-mode denial

**Type:** Refactor

Node 5 of the `#unbundle` stack ([#474](https://github.com/uppin/tddy-coder/pull/474)). Full story in
the cross-package entry:
[2026-09-10-unbundle-tools-thinning.md](../../../../docs/dev/changesets/2026-09-10-unbundle-tools-thinning.md).

`tddy-tools`' `server::exec_tool_catalog()` was a hand-copied `RemoteToolDef` clone of
`catalog::tool_catalog()`, with matched guard tests in both crates — the codebase knew about the
duplication and was paying to keep the two in step. **There is now one catalog**: `tddy-tools`
derives its `RemoteToolDef`s from `tool_catalog()` in eight lines at the single point that needs the
MCP shape, the way it already derives the `Lsp*` tools from `tddy_lsp_executor`. The advertised set
is byte-identical over the real `--mcp` stdio wire. `tddy-tools`' guard test went with the copy it
guarded; **`tddy-daemon`'s stayed**, because it guards a pair that has *not* collapsed — this catalog
against `tddy_sandbox::workspace_exec_tool_names`, the `--allowedTools` a sandboxed `claude` is
spawned with.

**`dynamic_proxy::is_native_tool_denied_in_remote_mode`** also arrived: `Write` / `Edit` /
`NotebookEdit` are hard-denied while the agent runs against a remote codebase, because the local
working directory is not the worktree being edited. A denial rather than an absence — the agent must
be told it was refused, and the replacement is this crate's own `Write` against the real worktree.

**The MCP shape stayed in `tddy-tools`** — `RemoteToolDef`, `build_dynamic_tool_list`,
`dynamic_tool_router`, `dispatch_dynamic_tool`. Putting it here would mean `rmcp` in a crate every
workspace-session host links, and `dispatch_dynamic_tool` resolves the call against the session's
live agent roster, which is a `tddy-service` concern. **Dependencies are therefore unchanged: no
`rmcp`, no `anyhow`.**

One red test was deleted rather than made to pass: `stops_advertising_a_tool_an_agent_has_taken_over`
required advertisement to be filtered by the roster. It is not, and must not be — `--allowedTools` is
fixed when `claude` spawns, so a takeover can only be enforced at dispatch.

Recorded in [README.md](../../README.md). `lib.rs` goes 624 → 703 non-blank lines; it was already
over budget before this node.

Tests: 11 / 0.
