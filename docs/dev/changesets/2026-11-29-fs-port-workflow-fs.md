# 2026-11-29 — The workflow, changeset and task crates go through the Fs port

**Type:** Refactoring · **PR:** [#1054](https://github.com/uppin/tddy-coder/pull/1054) (`#fs-port` 10/13)
**Packages:** `tddy-workflow`, `tddy-workflow-engine`, `tddy-workflow-recipes`, `tddy-changeset`, `tddy-tool-engine`, `tddy-task`, `tddy-actions`
**Base:** `feature/fs-port/sandbox-fs` (n9)

The production disk I/O of the seven crates goes through the `Fs` port. Session-file
formats (changeset, metadata, activity and tool-call logs, pending questions, tab rosters,
the start ledger, the action cache), the artifact writer and the workflow hooks delegate to
`<fn>_on` seams or held handles (`Arc<dyn Fs>` on `BackendInvokeTask`, the hooks structs,
`SessionWorkQueues`, `StartLedger`, `LocalShell`), with `DiskFs` only at composition roots:
old-name wrappers, `WorkflowEngine::new`, the hook constructors, and graph/test builders.
`WorkflowRecipe::build_graph` takes the port and threads it to every `BackendInvokeTask`;
`tddy-workflow` gained `resolve_existing_session_artifact_on`. Every crate carries
`tests/fs_port.rs` with the `direct_disk_io_in` guard over its own `src/`. Detail:
[the filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).

No `Fs` trait method was added on this node. Known gaps (recorded here rather than silently
left): `tddy-workflow-recipes`' merge-pr GitHub temp files are written for `curl` to read by
path, so they go through the disk port only; `tddy-changeset`'s `work_queue_fixtures`
(test-support) builds real temp layouts for other crates' tests through `DiskFs`; callers
outside the table (`tddy-presenter`, `tddy-cli-sessions`) pass `DiskFs` at their composition
sites per the changeset's Boundaries, for their own nodes to thread. Test-only crates keep
real disk.
