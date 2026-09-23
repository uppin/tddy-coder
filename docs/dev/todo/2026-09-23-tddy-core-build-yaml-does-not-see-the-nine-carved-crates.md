# 2026-09-23 — `tddy-core`'s `BUILD.yaml` does not see the nine crates carved out of it

**Category:** Future enhancement — tddy-build coverage
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found by `/validate-changes`

`packages/tddy-core/BUILD.yaml` declares one target, `tddy-core:lib`, with
`srcs: packages/tddy-core/src/**/*.rs` and `Cargo.toml`, and one dependency, `tddy-workflow:lib`.
After #522 that source glob matches six files: `lib.rs`, four facades and `ssh_exec.rs`. All the
code lives in `tddy-log`, `tddy-changeset`, `tddy-session-worktree`, `tddy-session-actions`,
`tddy-toolcall`, `tddy-agent-backend`, `tddy-agent-skills`, `tddy-workflow-engine` and
`tddy-presenter`, and none of those has a `BUILD.yaml`. A tddy-build cache keyed on
`tddy-core:lib`'s sources will not invalidate when any of that code changes.

**This gap predates #522.** The target already missed `tddy-session-store`, `tddy-session-catalog`,
`tddy-git` and `tddy-graph`, which `tddy-core` depends on. 30 of the workspace's 93 packages carry a
`BUILD.yaml`. #522 made the gap much wider, but did not open it.

**What would close it.** Give each carved crate a `rust_library` target and list them in
`tddy-core:lib`'s `deps`, or derive the Rust targets' dependency edges from `Cargo.toml` rather than
declaring them by hand. The second option closes the whole class.
