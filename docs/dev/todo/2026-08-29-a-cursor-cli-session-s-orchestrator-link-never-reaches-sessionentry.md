# 2026-08-29 — A cursor-cli session's orchestrator link never reaches `SessionEntry`

**Category:** Future enhancement
**Source:** subagent-conversation-inference changeset, 2026-08-29

- `cursor_cli_spawn.rs` writes `Changeset.orchestrator_session_id` for a session spawned under a
  stack parent (`packages/tddy-daemon/src/cursor_cli_spawn.rs:189`), but
  `session_list_status_from_session_dir` returns early for `session_type == "cursor-cli"` with
  `orchestrator_session_id: String::new()` before `changeset.yaml` is ever read
  (`packages/tddy-daemon/src/session_list_enrichment.rs:177`). The claude-cli arm above it does the
  same, and for claude-cli that is correct — it writes no changeset.
- Consequence: `agentTree.ts` folds the subagent tree out of `orchestrator_session_id`, so a
  cursor-cli child is not a node of it however healthy it is. **Raised stakes as of the
  agents-tab-subagent-tree changeset (2026-08-30):** the Agents tab's tree is now the *only* surface
  listing a session's subagents — `SessionAgentsSection` and `sessionPeers.ts` are gone — so a
  cursor-cli subagent is not merely missing from one section, it is invisible in the product. The
  [agent session status](../../ft/daemon/agent-session-status.md) work populates the status such a row
  would display, and is deliberately independent of it — the status lands on every agent session's
  `SessionEntry`, whoever groups them.
- Fix shape: read the changeset for the `orchestrator_session_id` (and `recipe`, which has the same
  gap) in the cursor-cli arm rather than short-circuiting past it. Needs a test that a cursor-cli
  session spawned under a stack parent lists with its parent's id.
