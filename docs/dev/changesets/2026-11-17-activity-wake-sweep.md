# 2026-11-17 — The activity-wake sweep splits the three files the stack grew past budget

**Type:** Refactor

The `#activity-wake` stack's sweep (`#activity-wake` 6/6, PR #936): the three files the stack's
PRs grew past the 500-production-line budget — each recorded by `/pr-wrap`'s file-length gate and
each deferred while a successor still touched it — are split into submodules by `move_item` /
`extract_module` through `tddy-tools restructure`. Nothing changes behaviour: 41 function
definitions moved in, 41 moved out, and every moved body is byte-identical to what it replaced.
The three `oversized-file-*` records are closed with their final measurements below.

## What landed

- `tddy-session-activity`: `session_watches.rs` (857 production lines) is a module with
  `matcher` (145), `delivery` (211) and `store` (349) submodules beside the pre-existing
  `stream` (428, from the #935 wrap); the parent keeps the kinds' constants and re-exports the
  moved surface so every external path is unchanged.
- `tddy-session-activity`: `project_notification_log/storage.rs` (578) lost its DTOs and their
  `From` conversions to `storage/dto.rs` (363).
- `tddy-daemon`: `mount_host.rs` (532) lost `BuildContext::open_shared` and its
  `open_session_watches` helper to `mount_host/open_shared.rs` (97).

## Final measurements (production lines, counted to the first `#[cfg(test)]`)

Closed, records deleted with this entry as their audit trail:

| File | Before the stack | At wrap |
|---|---|---|
| `tddy-session-activity/src/session_watches.rs` | 857 | 205 (matcher 145, delivery 211, store 349, stream 428) |
| `tddy-session-activity/src/project_notification_log/storage.rs` | 578 | 220 (dto 363) |
| `tddy-daemon/src/runtime/mount_host.rs` | 532 | 444 (open_shared 97) |

Left exactly as they are, not closed by this node:

- The function-ceiling side-notes the deleted `oversized-file-session-watches.md` record carried:
  `take_stream_batch` at 255 lines and `tddy-tools`' `monitor_cli::stream` at 62 against the
  60 ceiling. Closing either is `extract_method` work that rewrites bodies — outside a move-only
  sweep — and neither has a standing record of its own. Re-measured at this wrap and unchanged
  in kind (`take_stream_batch` ~255, `monitor_cli::stream` ~65 by brace match, both still over
  60); they are left for a human to plan.
- The pre-existing complexity records (`build_session_host` 73 lines,
  `stream_acp_replay` 80): re-measured, byte-identical to their master state, untouched by the
  moves.

## Verification

- Green before the first move, on the stack's tip (`8736ce533`), and green after the last, at
  this wrap: `./dev cargo test -p tddy-session-activity` and `./dev cargo test -p tddy-daemon
  --lib` (77 tests).
- `./dev cargo clippy -p tddy-session-activity -p tddy-daemon --all-targets -- -D warnings` and
  `cargo fmt --all --check` clean.
- `/analyze-code-issues` re-measurement at the wrap: no file the PR touches is at or over 500
  production lines; no function was added or rewritten (41 moved in, 41 moved out); the only
  open records in the two packages are the pre-existing complexity ones above, unchanged from
  master. The coverage-backed CRAP pass was skipped for this re-measurement: with no new or
  rewritten function body, every CRAP score is by construction unchanged, and an instrumented
  rebuild of both crates buys nothing the line and function-parity measurements don't already
  prove.
- The stack's one open feature TODO (`2026-11-16-session-watches-are-not-shown-on-the-web.md`)
  is not structural debt — it goes to the stack's TODO gate, not this change.
