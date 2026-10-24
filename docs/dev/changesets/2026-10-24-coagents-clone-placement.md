# 2026-10-24 — A subagent can work on its own clone of the session worktree

`#coagents` 5/8, [#724](https://github.com/uppin/tddy-coder/pull/724), on #723.
**Packages:** tddy-service (proto), tddy-changeset, tddy-session-agents, tddy-session-lifecycle,
tddy-coding-subagent, tddy-discovery, tddy-tools, tddy-agent-launch, tddy-acp-stub, tddy-web
(+ regenerated TS).

- Proto: `types.AgentPlacement`, `AttachSessionAgentRequest.placement = 5`,
  `SessionAgentEntry.placement = 15`, `StartSessionRequest.clone_placed_agents = 44` (the subset of
  `specialized_agents`); `SessionAgentRecord.placement` (serde default `Shared`).
- Open path: `clone_placement::open_session_clone` cuts the conversation worktree at open; a model
  agent's exec calls carry the conversation id (`run_in_conversation`) and its loop syncs through
  `ConversationWorktreeSync`; a coding agent gets `CodingAgentLaunch.conversation_id`, its cwd is the
  worktree and `mcp_servers_for` hands `TDDY_CONVERSATION_ID` to `tddy-tools --mcp`.
- `CodingAgentConversationTurns`: sync before each turn (honouring `syncWorktree: false`), one
  `Worktree::commit_changes` per turn (no new worktree-crate API), the sync notice prefixed to the
  next prompt through `TurnRequest::with_prompt_preface`.
- A re-attach under the other placement is `FAILED_PRECONDITION` before admission
  (`agent_placement::refuse_placement_change`), and the roster store refuses it too.
- `tddy-tools`: `worktree_route::has_its_own_worktree` decides diff/pull/end; daemon-run `clone`
  conversations use the existing `ConversationWorktree` RPC. `host_of` / `the_host_of` read the
  conversation and its queue under one lock.
- `tddy-acp-stub` scenario gains a `write_file` action.
- Web: placement select in `CreateSessionAgentPickerSection` and `AgentPicker`, placement on the
  roster row; the create form's picks live in `useSpecializedAgentPicks` (keeps `CreateSessionPane`
  at 494 lines).
- Accepted limits: a permission-answer turn carries no sync notice (it rides on the next prompt);
  the `tddy-tools` MCP process is not run end to end against a real daemon; `open_owned` and remote
  placement belong to `#coagents` 8/8.
- Code issues (stack-shared, split after the stack lands): session-agents `service.rs` 1,189 → 1,203,
  discovery `registry.rs` 805 → 806, discovery `subagent_runtime.rs` 740 → 745.
- Backlog: `2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`
  narrowed to the peer-owned remainder.

Feature docs: [managed-codebase subagents](../../ft/coder/managed-codebase-subagents.md#a-daemon-run-conversation-of-a-clone-placed-agent),
[session agent roster § Placement](../../ft/daemon/session-agent-roster.md#placement--shared-or-clone).
