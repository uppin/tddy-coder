# 2026-10-22 — service.rs split into ports, rpc and hook_reports

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/service.rs` **835 → 202**: the ports to `service/ports.rs` (146) and the whole `ActivityService`
  impl to `service/rpc.rs`, by `move_item`. That left `rpc.rs` at 509, so the two hook reports' bodies
  became inherent `report_session_status_impl` / `report_agent_activity_impl` in
  `service/hook_reports.rs` (215) by hand (a trait impl cannot be split, and `extract_method` is refused
  under `#[async_trait]`), the trait methods delegating: `rpc.rs` **509 → 323**.
- The record `oversized-file-service-rpc.md` was opened and closed inside this change (509 → 323), so
  none remains. `complexity-service-stream-acp-replay.md` moved to `service/rpc.rs`, unchanged at 80 lines.
- The backlog entry `2026-09-12-the-two-new-service-rs-files-are-over-budget` is narrowed to
  `tddy-session-agents`: this crate's `service.rs` row is closed along the seam it named.
