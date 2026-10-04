# The index-backed executor

`tddy_lsp_executor::index_backed` answers a session's `Lsp*` tools from the warm code-intelligence index
the daemon manages, instead of from a rust-analyzer of this crate's own.

Product behaviour: [Warm code-intelligence daemon](../../../docs/ft/coder/warm-code-intelligence-daemon.md)
and [Reusable LSP](../../../docs/ft/coder/reusable-lsp.md). The service it asks:
[`code-index-service.md`](../../tddy-index-daemon/docs/code-index-service.md).

## Surface

| Item | What it is |
|---|---|
| `IndexChannel` | An async port: `connect() -> tonic Channel`, starting the index if nothing has. The daemon implements it for `IndexDaemonRegistry` by delegating to `connect`, so the registry stays in `tddy-daemon`, which depends on this crate and not the other way round |
| `IndexLspExecutor` | An `LspExecutor` whose every method asks the index through an `IndexChannel`. Each call dials a fresh client |
| `bind_to_session_worktree(worktree, file)` | The host-side binding: the queried `file` as a path relative to the session's worktree, or a refusal |
| `select_lsp_executor(index, existing)` | The deployment switch: the index-backed executor when the host has an index, `existing` otherwise |

## One worktree, bound on the host

The `repo_dir` every `LspExecutor` method receives is the worktree the host resolved from the session
(or from the subagent's conversation worktree under it) — `tddy_tool_engine::execute_tool_with_env`'s
`worktree_root`. It is the index request's `workspace_root`. A path the jail supplies is never used as a
root.

`bind_to_session_worktree` is applied to the tool's `file` before the index hears of it:

| `file` | Result |
|---|---|
| Relative, inside the worktree (`src/lib.rs`, `./src/lib.rs`) | Bound relative, without a leading `./` |
| Absolute, inside the worktree | Made relative to it |
| Absolute, anywhere else — another session's worktree, or a sibling directory whose name merely starts with the worktree's | Refused |
| Containing `..`, or empty, or naming the worktree itself (`.`) | Refused |

A refusal is `"<file> is outside the session's worktree"`, and neither the index nor any other executor
is asked. The check is by the path's text, so a symlink inside the worktree that points outside it is
not caught here or by the service.

A workspace symbol search (`symbol_query` set) names no file and is not bound.

## What the tools see

The tools' names, argument schemas and result JSON are those of `TddyLspExecutor`: zero-based LSP
positions in and out, `file://` URIs, the same top-level keys (`locations`, `references`, `hover`,
`symbols`, `diagnostics`).

**Columns.** The tools speak LSP's UTF-16 columns and the index one-based byte columns. The conversion
belongs to this module and reads the line from disk, in both directions: the column a tool sends is
converted to a byte column before the index is asked, and the columns the index answers are converted
back (a line an answer falls on is read once per call). A file or line that cannot be read, or an answer past
the end of a line, is an error — never a guess. A column at or past the end of a line is its length, as
an editor clamps it.

**Locations.** An index location inside the worktree is joined onto it; one the index marks
`outside_root` is already an absolute path and is used as it is.

**Errors.** A failing index answers with its status message, which is the tool's error text. The
executor that would have answered without an index is not asked in its place.

## Availability and `ReadLints`

`is_available` is true when the worktree's root holds a `Cargo.toml` — the one language the index
serves.

`workspace_diagnostics` (the `ReadLints` tool) is **refused**: the index answers diagnostics for one file
at a time. The message points the agent at `LspDiagnostics` with a file. Answering it from a language
server of this crate's own would disagree with the index every other pane asks, so there is no
fallback. Backlog:
[`2026-10-04-read-lints-is-refused-through-the-warm-index.md`](../../../docs/dev/todo/2026-10-04-read-lints-is-refused-through-the-warm-index.md).

## Selection

`select_lsp_executor` is a deployment switch, not a fallback: with an index the executor passed as
`existing` is never asked. `tddy-daemon` registers its result with `register_lsp_executor` (first-wins
per process); see [`daemon-endpoint.md`](../../tddy-daemon/docs/daemon-endpoint.md).

## Dependencies

This crate depends on `tddy-index-daemon` for the generated `code_index` client, and on `tonic` and
`async-trait` for the port. `tddy-index-daemon` does not depend on this crate, so there is no cycle. The
dependency means every dependent of this crate — `tddy-session-lifecycle`, `tddy-sandbox-app`,
`tddy-tools` — links the index daemon's library, and with it `tddy-code-restructuring`. A thin
`code_index` client crate would remove that weight.

## Testing

`tests/lsp_tools_via_index.rs` runs every call through `select_lsp_executor` on a blocking thread, as
`tddy_tool_engine`'s `Lsp*` dispatch does, against a fake `code_index` server on an AF_UNIX socket
behind a test `IndexChannel` and a recording stand-in for the existing executor. The worktree fixture
includes a line with a multi-byte character and an emoji, for the column conversion. The binding is
unit-tested in `src/index_backed.rs`.

No daemon-level test proves that the runtime selects the index executor: the registration is
process-global and first-wins, and the runtime performs it before a test could observe it.
`select_lsp_executor` is the tested seam.
