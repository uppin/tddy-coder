# 2026-11-27 — Oversized files this change grew while already over

**Category:** Deferred refactor
**Source:** PR #1048 (`#fs-port` 4/13, lifecycle-fs) — `/pr-wrap` step 3.5, file-length gate

The fs-port migration grew two files that were already over the 500-production-line budget, by the
minimum each seam needed. Neither is split in #1048: that PR's decomposition scope is
`session_action_pipeline.rs` (435 → 514, the one file this PR pushed *past* the budget), and a
second and third engine split in the same PR would bury the port diff under mechanical moves. No
other open `#fs-port` node touches either file (checked #1047 and #1049–#1057 against `master` on
2026-11-27), so the sweep node can cut them without meeting an in-flight diff.

| File | Production lines | Cause |
|---|---|---|
| `packages/tddy-session-agents/src/session_agent_clone.rs` | 1,156 → 1,159 | the mirror-marker write goes through `tddy_fs::DiskFs` (+4/−1) |
| `packages/tddy-session-split/src/svc_spawn_split_agent.rs` | 514 → 522 | the `*_on` port variants with their `DiskFs` composition sites (+8, no logic) |

Each file has a code-issue record (`oversized-file-*.md`, both unclaimed) that now carries the
measurement.

**Recommendation per file (own):**
- `session_agent_clone.rs` — split in the sweep node, not here. The record's seam (lifecycle create /
  ready / retire vs the hosted-clone serving path) is still undesigned; prove it with `restructure
  check --deep` before applying. The port growth is import/comment plus a longer call, so the seam
  the sweep cuts is the pre-existing one, unchanged by this PR.
- `svc_spawn_split_agent.rs` — split in the sweep node, not here. First seam to try: the `*_on`
  port twins (each ported entry point is now a `DiskFs` wrapper plus the `fs`-taking body) moving as
  one topic. Only 22 lines over, so a small extraction suffices — but not inside #1048, where it
  would mix with the port's own review.
- `packages/tddy-session-actions/src/session_action_pipeline.rs` (435 → 514) — **not deferred**:
  decomposed inside #1048, since this PR pushed it past the budget.
