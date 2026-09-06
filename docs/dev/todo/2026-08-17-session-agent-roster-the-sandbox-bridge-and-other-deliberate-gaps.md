# 2026-08-17 — Session agent roster — the sandbox bridge, and other deliberate gaps

**Category:** Future enhancement
**Source:** session-agent-roster changeset, 2026-08-17

- ~~**⛔ BLOCKER — `subagent_*` is refused inside a managed jail, so the feature does not work in a
  sandboxed session.**~~ **Closed 2026-08-29.** Both halves are built. The bridge:
  `tddy_sandbox_runner::ToolExecService` (`packages/tddy-sandbox-runner/src/runner.rs`) now
  forwards `StreamSessionAgents`, `OpenAgentConversation`, `PromptAgentConversation` and
  `CancelAgentConversation` over the `SessionChannel` as multiplexed `RpcRequest` /
  `RpcStreamFrame` frames carrying a `request_id`, alongside — not through — the poll-gated
  `ToolRequest`/`ToolResponse` pair `ExecuteTool` still rides, so a lifetime-long roster stream no
  longer occupies the channel. The client: `tddy-tools` calls the three conversation RPCs on the
  facilitating daemon (`session_agents/conversation.rs`), so an agent whose def only its owning
  daemon holds — a live attach, or a cross-daemon agent on a split session — is run by that daemon
  instead of being refused. The three blockers below are the state before that, kept for the
  record.
  1. `SandboxSessionRelay` held a **single** `awaiting_tool` slot popped positionally, so a
     lifetime-long `StreamSessionAgents` would occupy the channel forever and block every tool
     call. Multiplexing needed a request id on `SessionFrame` and an in-flight map on both sides.
  2. `SessionFrame` carried one typed message per call, not a
     `{request_id, service, method, payload}` + `end_of_stream` envelope.
  3. The host side dispatched to `HostToolHandler::execute(session_id, tool_name, args_json)`,
     which deliberately does not carry the daemon's `ConnectionServiceImpl` — the conversation RPCs
     need it.
  The capability half was closed earlier: `LiveAgentRoster` distinguishes a roster that *was*
  current and went stale (`RosterCurrency::Stale` — still enforces its `replaces` union, because
  those agents were real) from one that never received a frame and whose stream gave up
  (`RosterCurrency::Unreachable` — enforces nothing, because the replacement is unreachable too),
  so a session whose roster never arrived keeps `Grep`/`Glob` at the call site instead of losing
  search entirely.
- **The sandbox runner re-derives the replaced-tool set from the spawn seed, not from the roster.**
  `runner.rs` reads `TDDY_SUBAGENTS_JSON` and feeds `append_claude_mcp_args`, while the daemon's
  `roster_replacement_pairs` and the context appendix it feeds use the roster's snapshot of
  `replaces`. For
  an unedited def the two agree. They diverge exactly in the case the snapshot-at-attach design
  exists to prevent: a YAML def whose `replaces` is edited between attach and relaunch. Closing it
  means the daemon handing the runner the roster's replaced set outright, which changes the runner's
  env contract — and `tddy-sandbox-app`, which has no daemon, must keep deriving from the seed.
- **Detach cancels no conversation, local or remote.** `cancel_conversations_with`
  (`connection_service.rs:3988`) `retain`s the `Remote` routing record out of the *local* map and
  never sends `CancelAgentConversation` to the owning daemon, so the peer's session keeps running
  and stays registered in its own `agent_conversations` forever; an in-flight forwarded
  `PromptAgentConversation` is not aborted, and the caller can still receive a completed answer for
  an agent that is no longer attached. A local conversation records no agent id at all, so it is not
  even matched. AC15 is therefore unmet on both paths. The code comment at that site claims the
  remote path works — it does not.
- **Conversation RPCs ride the common room, not the session room.** The PRD says the owning daemon
  serves its agent surface in `session-{id}`; it does join and mirror there, but A→B conversation
  forwarding uses the common-room peer path. Serving a second RPC surface on the session-room
  connection needs `LiveKitParticipant` to expose its room as a shared handle so one connection can
  both serve and subscribe — it does not, and a second connection under the same identity is one
  LiveKit disconnects.
- ~~**No room-admission handshake exists.**~~ **Closed 2026-08-18.** The handshake is built:
  `SessionAdmissionRegistry` records admitted owning daemons per session; `provision_agent_clone`
  mints a scoped 5-min token and forwards it in `AgentClonePlacement.first_admission_*`;
  `SessionAdmissionService.AdmitOwningDaemon` (served on A's common-room participant) refreshes
  it; `run_clone_mirror` runs a re-admit loop (full-rejoin) on room disconnect; revocation is
  wired in `tear_down_agent_clone` (last detach) and `delete_session`
  (`revoke_all_for_session`). The PRD § "What attach does" step 3 has been rewritten to match.
- **A hosted clone's exec-tool path authenticates by the clone link, not a user token.** Documented
  at `run_hosted_clone_tool`. A session-scoped tool token (audience = this session, exec-tool
  methods only) is the intended fix and is the same open item
  [remote-managed-worktree.md § Trust model](../../ft/daemon/remote-managed-worktree.md) already
  records for split sessions. Note this is *separate* from the missing authentication on that path,
  which is a defect being fixed rather than accepted debt.
- **`docs/ft/coder/specialized-subagents.md` contradicts the code in five criteria.** Its criteria
  2, 3, 10, 13 and 14 still describe `builtin_fastcontext_def()`, the back-compat
  `TDDY_SUBAGENT=fastcontext` path, `FastContextBackend` and a clap allowlist — none of which exist
  after this branch. It is superseded by
  [session-agent-roster.md](../../ft/daemon/session-agent-roster.md) and needs either a rewrite or an
  explicit "superseded by" banner.
