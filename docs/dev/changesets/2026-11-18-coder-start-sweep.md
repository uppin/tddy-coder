# 2026-11-18 — The coder-start stack's structural debt is paid down

**Type:** Refactor · `#coder-start` 4/4, PR [#944](https://github.com/uppin/tddy-coder/pull/944)
**Packages:** `tddy-remote-shell`, `tddy-agent-launch`, `tddy-session-driver`

The `#coder-start` stack (#939, #941, #942) left three code issues. This node closes two and narrows
the third by engine moves and extractions through `tddy-tools restructure`; no moved or extracted code
was written by hand, and nothing public changed.

| Record | Before | After | Budget |
|---|---|---|---|
| `tddy-remote-shell` `coder.rs` production lines | 507 | **405** (`coder/one_shot.rs` 107) | 500 |
| `tddy-agent-launch` `launch_on_target` | 184 | **59** (8 helpers, each at most 48) | 60 |
| `tddy-session-driver` `DriverSession::launch` | 61 | **58** (`wait_out_the_startup_window` 6) | 60 |

- Records deleted with their final measurement here: `oversized-file-coder`, `complexity-session-launch`.
- Record narrowed, still open: `launch_on_target` keeps its 6 parameters (budget 5). Bundling them
  changes the public signature, which a Refactor does not do.
- The pid-directory `exec` stays inline in `launch_on_target`: rust-analyzer's extraction rewrote
  `ExecRequest::new(format!(..))` as `ExecRequest::new(*format!(..))`, which does not compile; the apply
  rolled back and `plan_the_launch` and `client_argv_and_env` took its place in the budget.
- The engine's lift of an `async` guard (`if … { return Err(…) }`) is not recognised; the tail-range
  extraction keeps those `return`s verbatim instead.
- Tooling tidy: one `cargo clippy --fix` for a `let_and_return`, and a hand-written simplification of
  the tuple rebinding in `client_argv_and_env`.

Verified, scoped: `./test -p tddy-remote-shell -p tddy-agent-launch -p tddy-session-driver -p tddy-tool-engine`
692 passed, 0 failed; clippy `-D warnings` on the three packages and `cargo fmt --check` clean; the
comment-line multiset of the touched sources is identical before and after. Workspace health is CI's.

Per-package notes: [remote-shell](../../../packages/tddy-remote-shell/docs/changesets/2026-11-18-coder-start-sweep.md),
[agent-launch](../../../packages/tddy-agent-launch/docs/changesets/2026-11-18-coder-start-sweep.md),
[session-driver](../../../packages/tddy-session-driver/docs/changesets/2026-11-18-coder-start-sweep.md).
