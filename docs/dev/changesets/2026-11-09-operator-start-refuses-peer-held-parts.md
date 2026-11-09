# 2026-11-09 — An operator start refuses peer-held parts, and a refused delete leaves the session intact

**Type:** Bug Fix

`#agent-tools` 30/32 ([#858](https://github.com/uppin/tddy-coder/pull/858)).

`start_session_for_os_user` refuses `INVALID_ARGUMENT`, naming the field, an operator start that sets `codebase_daemon_instance_id`, `sandboxed_codebase`, `specialized_agents` or `clone_placed_agents` (`refuse_peer_held_parts`), before anything is created. An operator start's undo is keyed by its OS user and carries no session token, so it may not create a part only a token-authenticated peer delete could remove.

`delete_session_as` runs `refuse_a_delete_a_peer_would_refuse` before the tmux kill and the pointer removal. It refuses `FAILED_PRECONDITION`, with the session and its agent still running, a codebase half on another daemon (`SplitSessions::refuse_an_unreachable_paired_codebase`) or a recorded agent clone (`AgentRoster::refuse_an_unreachable_clone_host`) while no common room is configured or joined. A peer reachable at the check that fails at its delete still fails after the local teardown began; running the peer deletes first was the alternative, not chosen.

Packages: tddy-agent-launch (`operator_start.rs`, `start_task.rs`, `svc_signal_delete_session.rs`), tddy-session-split (`svc_paired_codebase_teardown.rs`), tddy-session-agents (`svc_provision_agent_clone.rs`), tddy-session-lifecycle (tests only: `refused_delete_keeps_the_session_acceptance.rs`).

Backlog: `docs/dev/todo/2026-11-07-operator-start-undo-of-peer-held-parts.md` **closed as unreachable** (user, 2026-11-07) and deleted, with `TODO(operator-start-undo)`: an operator start can no longer create a peer-held part, so its undo never needs a peer delete. The record's OS-user-keyed peer delete is dropped.

Code-issue measurements: no record names a touched function; `complexity-svc-paired-codebase-teardown-delete-paired-codebase-session.md` and `complexity-svc-provision-agent-clone-provision-agent-clone.md` cover neighbouring functions this PR did not change. Touched production files stay under 500 lines (largest: `svc_provision_agent_clone.rs`, ~415).
