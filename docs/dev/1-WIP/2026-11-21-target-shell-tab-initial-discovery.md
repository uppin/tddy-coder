# Initial discovery — target-shell-tab (#session-liveness n6)

## Exploration 1 — whole-work discovery (copied)

Temporary whole-work dump (plan-pr-stack Step 2). Copied into each node's `-initial-discovery.md`; not committed on a stack branch.

## Combined conclusions

- Today a sandboxed session's secondary tab is an **unjailed host PTY** (`CliManagerTerminalRoster::start`, svc_terminal_ports.rs:301), and it is **unreachable while the jail is registered**: `DaemonTerminalSessionStore::get_terminal` returns None for any non-main id once `sandboxes.get(session_id)` is Some (terminal_session_adapter.rs ~395-415). A sandbox node must fix this routing.
- `StartSessionRequest` makes `sandbox` and `target` mutually exclusive (session.proto:860-867): the two features never meet in one session.
- Wire: `StartTerminalSessionRequest.wrappers` (repeated LaunchNode) already exists; `TerminalKind::from_wrapped_request` (tddy-terminal-rpc/src/service/ports.rs:72-170) refuses every wrapper but `tmux`. `CoderTerminalRoster` (participant) refuses all but bash — needs refusal arms.
- Recording: tddy-changeset tab_roster.rs kinds agent/shell/tmux/tmux-attach; unknown nodes kept verbatim (serde flatten). Restore: restore_tabs/plan.rs `is_known_kind` + outermost-kind dispatch; unknown kind anywhere drops the chain. `RosterRestore` has no sandbox manager, no RemoteShell, no target.
- Sandbox main: `LaunchSessions::launch_jailed` (jail_launch.rs, 151-line fn), plan rebuilt per launch by `lower_tool_jail` (no persisted profile; `.session.yaml` has only `sandbox: Option<bool>`). Registry `SandboxSessionManager` = HashMap keyed by session id (insert overwrites → Drop kills). Runner frames hard-code `terminal_id: main`; ipc socket `tmp/tddy-<fnv(session_id)>-<pid>.sock` and `<session>/sandbox/{sandbox.ready,exec-plan.json,sandbox.sb}` are per session → a second runner collides (and launch_jailed deletes socket/marker first). Egress dir/log keyed per session. Persistent agent homes may race between two live jails. `SandboxTerminalSession` is non-resizable, no control lease, no ack offsets.
- No port for the main-jail spawn; tests build `SandboxSessionState` over a stand-in child (`a_sandbox_running("sleep",...)`). Real-jail suites are macOS-only (several failing) or Linux arms; CI runs no Linux jail (todo 2026-11-02-ci-runs-no-linux-jail).
- Liveness: CLI manager counts non-bash/tmux terminals; `SandboxSessionManager::live_sessions` one per session. Teardown points for tab jails: close_tab/end, delete (forget_session_state), store stop_session, resume, shut_down_children (which today does NOT stop sandbox_manager jails — todo 2026-09-15), tab exit/SessionEnded reap.
- Target: `shell_for_session_target` (ssh + coder, needs user_resolver + coder_credentials, today only in LaunchSessions); `launch_on_target` is main-only (pid file keyed by session id, pointer rewrite, startup window that stop_session()s the whole session) — a tab must use a `cd <wt> && <login exec>` + `shell.interactive` + a non-main variant of `spawn_rendered_client_main`. Target tabs are local PTYs → existing liveness/stop apply. Agent target tab hits todo 2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host (new_agent_tab resolves binary + mints chat on host).
- Web: NewTerminalMenu.tsx ("Agents in tmux" group pattern), `useSessionTerminals.open(kind, tmux, wrappers)` sends wrappers `{kind}` only; `SessionEntry` has `target` but no `sandbox` field.
- Records in path: oversized sandbox_session.rs (924), heavy-params dial_and_bridge (12), complexity launch_jailed (151), launch_on_target params, spawn_tool_on (13 params), runner.rs/host_relay.rs oversized, oversized test_util.rs; todos 2026-09-15 orphans-sandbox-children, 2026-11-19 target-requires-agent-on-host, 2026-11-19 coder-remote-agent-local-paths, 2026-11-16 remote-shell fixtures duplicated, 2026-11-02 jailed-claude-loads-project-mcp-json, 2026-10-24 claude-agent-tab-shares-session-hooks.

## Exploration 1 — code map (Explore agent, 2026-11-20)

See conclusions above; full report kept in the orchestrator transcript. Key file:line anchors: recorded_tabs.rs:36-57 (start_tab), :150-155 (new_agent_tab), :209-228 (record_started); plan.rs:95-131; roster.rs:28-40,49,~110,~155; sandbox_session.rs:31-75,135-176,484,537,824,883-899; jail_launch.rs:~95-125,~135-290,~202-205,~395-436; runner.rs:274-330,350,357,~690-780,~749-760,~814; host_relay.rs:~841-857; remote_agent_launch.rs:~57-87,~227-270,~421,~442; session_start.rs:94-115; terminal_session_adapter.rs:~165-230,~395-415; NewTerminalMenu.tsx:25,~140,~187-206; useSessionTerminals.ts:~111,~151-178; terminal_session.proto:199-222,261.

## Interview (2026-11-20)

- n5 sandbox tabs: a **second jail per tab** (own tddy-sandbox-runner), same session worktree + same sandbox profile (egress, mounts, tool relay), own process tree/PTY/lifetime.
- Per-tab choice in the new-tab menu ("in sandbox" / "on host"); default for a sandboxed session = jail; agent and shell tabs alike. Recorded per tab as a `sandbox` wrapper node in tabs.yaml; mixed rosters allowed.
- Sandboxed agent tab = live agent tab (n1 rule); stop/delete/pause stops every tab jail; exited tab jail leaves the roster.
- Tests: behaviour against an in-memory sandbox spawn port; real-jail tests Linux-only on CI; no new macOS-harness dependence.
- n6 target tabs: one `target` wrapper kind (properties: transport ssh alias / coder workspace, worktree); new tabs of a target session open on the target by default ("on target" / "on this host"); run via RemoteShell.interactive in the target worktree; restored the same way; dropped with a reason when unreachable.
- Order: n5 sandbox, n6 target; both on top of n4 (#961).
