# 2026-08-30 — PR-Stack: `QueryBranch`'s local-only legs

**Category:** Future enhancement
**Source:** 2026-08-30-cross-host-planned-pr-visibility changeset

- **The `worktree` and `remote` legs still answer only for the daemon that was asked.** `worktree` is
  `git worktree list` in this daemon's checkout and `remote` is `git rev-parse
  refs/remotes/origin/<branch>` against this daemon's remote-tracking refs, so a branch created on
  another host reads absent from both, with no `unavailable` discriminator to say so. Force start
  (D42) covers the operator-visible half; the residual is that a cross-host node's row shows no
  worktree path and no remote sha. A real fix means routing `QueryBranch` per branch to whichever
  daemon owns the checkout — a per-branch fan-out on a 5 s poll — or giving both legs the
  `unavailable` discriminator the `pr` and `base_sync` legs already have.
- **`resolveRepointTarget` skips a parent whose branch reads absent from this host's origin**, so a
  repoint can name the default branch where a parent would have served. The target is stated on the
  button before the click (D18), so nothing happens behind the operator's back, but the offered
  target is wrong for a cross-host stack.
- **The daemon-side `session` metadata block for a claude-cli session carries static fields only.**
  It is seeded at spawn with the identity and association fields and re-published unchanged on a 30 s
  timer (so a failed `set_metadata` or a room rejoin does not lose it); `activity_status`,
  `workflow_goal` and the rest stay empty, as they are today. Wiring the claude-cli hook stream into
  a metadata watch would make cross-host drawer rows for these sessions as live as tddy-coder's.
- **A sandboxed claude-cli session publishes no block at all.** The darwin Seatbelt path joins no
  LiveKit room, so it creates no participant to advertise on: "every claude-cli session now publishes
  the block" holds for the interactive path only, and a planned PR's child started sandboxed is still
  invisible to a PR-Stack view on another host. The association it would carry is written on the
  orchestrator's node by `LinkStackNode` regardless, so the same-host join is unaffected.
- **A sandboxed cursor-cli session still records nothing on its planned node.**
  `start_sandboxed_cursor_cli_session` now receives `stack_node_id`, so its *base* gate resolves
  correctly, but it makes no `link_spawned_branch` call at all — the same gap the interactive
  cursor-cli path had before this changeset closed it. A planned PR started that way leaves the node
  branchless and childless, which keeps `Stack::base_ref_for_spawn` refusing every descendant.
  Pre-existing; the fix is the three lines `cursor_cli_spawn.rs` gained, applied to the sandboxed
  sibling once it reads its branch back the same way.
