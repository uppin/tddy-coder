# Agent clones and the roster's daemon side (`tddy_session_agents`)

The daemon-side topic code of a session's agent roster runs over `AgentRoster`, an owned handle over the
host's roster fields, not over the daemon host. About 6.6k production lines in the crate.

`agent_host_callbacks.rs` defines `trait AgentHostCallbacks` (what the topic needs from the host that is
not a field), the `AgentRoster` handle with its `state()` (a borrowed `AgentRosterState<'a>` of the twelve
roster fields: `config`, `tddy_data_dir`, `user_resolver`, `peer_routing`, `room_roster`, `session_rooms`,
`session_agent_rosters`, `session_agent_clones`, `hosted_agent_clones`, `roster_keepalive_interval`,
`session_admissions`, `model_registry`) and `DaemonSeedCloneClaimant`, which holds the handle and which the
launch topic hands to the roster as its `SeededAgentClones`. A `self.clone()` handed to a task clones the
handle. The daemon host implements the callbacks once, in `tddy-session-lifecycle`.

| Module | Holds |
|---|---|
| `svc_provision_agent_clone.rs` | `AgentRoster`'s provisioning and tear-down of agent clones, the roster broadcast and publish, and the clone and worktree lookups (12 methods) |
| `svc_start_hosted_agent_clone.rs` | starting a hosted clone, the unready and departed-daemon refusals, the forwarded and local conversation opens, and the clone's codebase access (9 methods) |
| `svc_ensure_session_room_for_agents.rs` | claiming, seeding and unwinding a clone's roster entry, and `ensure_session_room_for_agents` (6 methods) |
| `svc_resolve_listed_worktree.rs` | agent-def resolution (`resolvable_agent_defs`, `agent_def_for_spawn`, `resolve_specialized_agent_defs`), `seeded_roster_records`, `roster_session_dir`, `report_shadowed_agent_def` (6 methods) |
| `svc_turn_end_reporter.rs` | the cancel forward to an agent's owning daemon and the local and remote roster records for an agent id (4 methods) |
| `agent_roster.rs` | the free roster functions (`workspace_start_request`, `started_roster_rev`, `dispatch_envelope`, `refuse_unenforceable_withdrawal`, `session_enforces_a_withdrawal`, `roster_agent_ids`, `agent_tool_reads_the_clone`), the `agent_records` re-export and `split_forward_deadline(config)` |
| `peer_session_answer.rs` | the free items that read or classify a peer's answer about a session: `peer_has_no_such_session` (a peer's `FailedPrecondition` or `NotFound`), `split_pairing`, `resolve_worktree_root_in_session_dir`, `resolve_worktree_root_for_session` |
| `seeded_clone_guard.rs`, `seed_codebase.rs`, `roster_replacement.rs` | the guard that releases a seeded roster on drop, `SeedCodebase` (the codebase a starting session seeds its roster from), and the roster's withdrawals (`roster_replacement_pairs`, the one source of what a session's roster withdraws, used by every sandboxed spawn and relaunch) |
| `session_dir_lookup.rs` | `session_dir_for(tddy_data_dir, session_id)` |

The host-free pieces the methods call (`clone_readiness`, `agent_clone_lookup`, `agent_clone_worktree`,
`conversation_cancel_forward`, `conversation_open_forward`, `departed_daemon`, `session_room_participants`,
`roster_broadcast`, `opened_session_room`, `spawn_agent_def`, `hosted_clone_start`, `agent_records`,
`exec_tool_caller`) are modules of this crate as well.

The crate depends on `tddy-subagent-worktree` (the roster resolves an agent's own worktree) and on
neither `tddy-session-split`, `tddy-agent-launch` nor `tddy-cli-sessions`. Standing findings are in
[`code-issues/`](code-issues/).
