# 2026-11-21 — `AddLspRoot` warms a root with an empty symbol query instead of `code_index.Warm`

**Category:** Technical debt (left by `#orchestra` 8/9, #986)
**Source:** `packages/tddy-daemon-rpc/src/exec_tool/lsp_roots.rs` (`warm`)

`AddLspRoot` starts loading an added root's index by sending an empty `LspSymbols` query through the
registered `LspExecutor` (`tddy_core::toolcall::lsp::lsp_executor()`), on a blocking task, and ignores
the answer. It is best effort: the trait has no warm call, and an empty query is the cheapest one that
makes the index daemon load the root. A host with no executor, or one that does not serve the root,
loads it on the first real query.

The real warm path exists: the index daemon's `code_index.Warm` RPC, which
`tddy_daemon_rpc::code_index_warmup` (`warm_for_session_with_supports`) already drives for a session's
own worktree when it starts. It is not reachable from `ExecToolRpcHandler`: the handler holds no
`IndexChannelSource`; only the session host's `IndexWarmupObserver` and `code_session` are given one,
in `tddy-daemon`'s `mount_host.rs`.

**Why deferred:** reaching `Warm` means threading the daemon's `IndexChannelSource` (and its
`SupportRegistry`) into `ExecToolRpcHandler` from `tddy-daemon`'s wiring — a host-construction change in
a package `#orchestra` 8/9 does not otherwise touch — or adding a `warm` method to the `LspExecutor`
trait in `tddy-core` and its every implementation.

**What would close it:** give `ExecToolRpcHandler` an optional `Arc<dyn IndexChannelSource>` (plus the
supports registry) where `mount_host.rs` installs `IndexWarmupObserver`, and have `lsp_roots::warm`
call `code_index_warmup::warm_for_session_with_supports` for the added root; drop the symbol query and
its `TODO(docs/dev/todo/2026-11-21-add-lsp-root-warms-by-symbol-query.md)` marker.
