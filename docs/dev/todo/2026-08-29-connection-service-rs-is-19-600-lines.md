# 2026-08-29 — `connection_service.rs` is 19,600 lines — resolved 2026-09-09

**Category:** Future enhancement
**Status:** Resolved
**Source:** subagent-conversation-inference changeset, 2026-08-29

**Resolved 2026-09-09** by the `#unbundle` stack's root node, [#470](https://github.com/uppin/tddy-coder/pull/470). Two cuts, in order: PR #468 turned
the file into a 2,416-line facade over 60 modules, and #470 took the first group *out of the crate
entirely* — 17 of `ConnectionService`'s 90 methods became `host.HostService` and
`worktree.WorktreeService`, in `packages/tddy-host-service` and `packages/tddy-worktree-service`.

The prediction below was right and had to be sharpened: **`pub(crate)` does not cross a crate
boundary**, so each subsystem's state moved behind an owned struct (`HostServiceImpl`,
`WorktreeServiceImpl`) with `with_*` builders, which the 30 existing `pub trait` ports made
affordable. Nine widenings still had to stand, each forced by a caller that stayed behind; they are
listed in [the changeset](../changesets/2026-09-09-unbundle-host-worktree-services.md).


- Pre-existing, and long past any sane module limit. Flagged here rather than acted on: this
  changeset adds 44 lines to it, and a split would bury a reviewable feature under a 19,000-line
  move — the same trade #418 recorded for `tddy-tools/src/server.rs`.
- Cohesive groups a split would follow, by responsibility rather than line range: session lifecycle
  (start / resume / delete / repoint), the agent-roster and agent-conversation RPCs, the streaming
  replay handlers (`StreamAcpReplay`, `StreamSessionActivity`, terminal), the PR-stack and changeset
  mutations, and peer routing / forwarding. Each is a plausible module.
- Cost: `ConnectionServiceImpl`'s ~60 private fields would have to become `pub(crate)` or move
  behind accessors, and every `#[cfg(test)] mod tests` block in the file (several hundred tests)
  would repoint. That is the reason nobody has done it, and the reason it needs to be its own PR.
