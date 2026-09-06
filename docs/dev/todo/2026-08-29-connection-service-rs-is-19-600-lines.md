# 2026-08-29 — `connection_service.rs` is 19,600 lines

**Category:** Future enhancement
**Source:** subagent-conversation-inference changeset, 2026-08-29

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
