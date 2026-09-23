# 2026-09-23 — The carved crates' manifests repeat dependency versions and the tokio feature list

**Category:** Future enhancement
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found by `/analyze-clean-code`

Each of the nine crates carved out of `tddy-core` declares its external dependencies with its own
version and feature list, copied from `tddy-core`'s manifest. Across the nine:

| Dependency | Declarations |
|---|---|
| `log = "0.4"` | 8 (plus 1 with `features = ["serde"]`) |
| `serde = { version = "1", features = ["derive"] }` | 7 |
| `serde_json = "1"` | 6 |
| `tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros", "fs", "process", "sync", "net", "io-util", "time"] }` | 3, identical |
| `chrono = { version = "0.4", default-features = false, features = ["clock"] }` | 3 |

Measured with
`grep -hE '^(tokio|serde|serde_json|log|chrono|uuid) *=' packages/tddy-{log,agent-skills,changeset,session-worktree,session-actions,toolcall,agent-backend,workflow-engine,presenter}/Cargo.toml | sort | uniq -c`.

**Why it was left.** The root `[workspace.dependencies]` holds only three entries (`rstest`,
`pretty_assertions`, `sysinfo`), and no crate in the workspace inherits a runtime dependency from
it. Converting only the nine carved crates would start a second convention inside a move-only PR.

**What would close it.** Add the shared ones to `[workspace.dependencies]` and have the carved crates
use `<dep>.workspace = true`. The whole tokio feature list in particular should live in one place,
since the three copies drift independently. Worth deciding for the whole workspace rather than for
these nine alone.
