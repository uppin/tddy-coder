# 2026-10-17 — `restructure apply` never half-moves a file, gates a move's destination, and ends a silent wait

- **An untracked move source is refused before anything is written.** A move of a file git does not
  track is refused as `UntrackedMoveSource`, naming the file and saying to `git add` it, before the
  operation writes a text edit or a journal record. After `git add`, the same plan applies with no
  `--resume` and no clean-up. `apply --dry-run` and `check --deep` refuse it the same way, and static
  `check` reports an untracked anchor file of a cross-crate move.
- **The baseline compile gate covers each cross-crate move's destination**, so a destination that was
  already broken is reported as `cargo check --all-targets -p destination -p origin` failing before the
  plan runs, not blamed on the plan afterwards. A `to` with no manifest yet adds nothing.
- **A silent wait ends in a named refusal.** A ready index that leaves a position untyped for 30 s is
  refused as an unusable answer naming `file:line`, for every wait for type inference. A server that
  says it is loading and then says nothing new for 10 minutes is refused as `ServerStalled`
  (`DeadlineExceeded` over the wire), naming the stage — the crate-index warm-up or type inference —
  and its last words. A load that keeps narrating is waited on: there is still no total deadline. The
  heartbeat says `is ready and gives no answer here` for a ready server.
- **Spawn records name their operation.** `start` lines carry `op` and `op_id` for the processes an
  operation starts, and `group` for a group's end-of-group `cargo check`; the command line's
  `op N (<id>) of M: resolving …` names the id.
- **A stale plan says a run wrote it.** A plan refused as stale after its own run says that run wrote
  the plan file, names the last operation it applied and the journal, says whether its edits were
  undone, and says to regenerate the plan with `restructure anchors`.

See [Rust code restructuring](../rust-code-restructuring.md#waiting) and its
[refusal classes](../rust-code-restructuring.md#refusal-classes).
