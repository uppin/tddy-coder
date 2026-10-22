# 2026-10-22 — runtime.rs becomes a module directory and runtime::build a BuildContext of stages

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/runtime.rs` **1,867 → 296** production lines. `RuntimeTasks` and its children moved to
  `runtime/tasks.rs` (312), `apply_env_overrides` and the data-directory rule to `runtime/env.rs` (91),
  `build_telegram` and `split_room_hosting` to `runtime/telegram_wiring.rs` (149), by `move_item`.
- `runtime::build` **1,080 → 14 lines**: by hand, with the developer's consent (`TODO(restructure-engine)`
  in `runtime/mount.rs`), around `BuildContext` (`runtime/build_context.rs`, 164) and its stages —
  `identity.rs` (200), `integrations.rs` (187), `mount.rs` (320), `mount_host.rs` (307),
  `mount_entries.rs` (293), `mount_accounts.rs` (124), `mount_socket.rs` (91). Every stage method is at
  most 58 lines and 5 parameters; each body is a verbatim slice of the old function bound to its old
  local names. A read-only review confirmed every statement appears once, side effects in the same order.
- Records closed and deleted: `oversized-file-runtime.md` (1,867 → 296) and `complexity-runtime-build.md`
  (`build` 1,080 → 14 lines).
- `tests/unbundle_endpoint.rs`: the module whitelist names the eleven `runtime/` paths, and the crate's
  non-blank line cap is **6,500** (was 6,000). The split adds about 630 non-blank lines of plumbing (the
  context struct, each stage's imports and the prologues binding the context's fields), taking the crate
  from 5,707 to about 6,340 on top of `#provider-route` 1/9; the whitelist, not the count, is what keeps
  session logic out.
- `tddy_daemon::runtime::apply_env_overrides` is no longer reachable from outside the crate (no caller
  outside `runtime` named it); every other path a caller names resolves through `runtime.rs`.
