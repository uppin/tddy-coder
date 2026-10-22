# 2026-10-22 — participant.rs split into publisher and serve

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/participant.rs` **907 → 418**: `SharedPublisher` and the response drains to
  `participant/publisher.rs` (170), the serve loop to `participant/serve.rs` (348) by `move_impl_members`.
- Record closed and deleted: `oversized-file-participant.md` (907 → 418). `complexity-participant-run.md`
  moved to `participant/serve.rs`, unchanged at 198 lines.
- `cargo test -p tddy-livekit -- --list` names the same 83 tests at the merge-base and after.
