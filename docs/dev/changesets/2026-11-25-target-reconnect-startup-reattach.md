# 2026-11-25 — Daemon start re-attaches tmux-run target mains and agent tabs (node 3/3)

**Type:** Feature

Node 3 of the `#target-reconnect` stack (node 1, the `TmuxLocation` port, merged as #1040; node 2,
target exit and lazy re-attach, merged as #1041). This PR was repointed onto `master` after both
merged, with its base set to `master`.

`tddy-session-lifecycle`'s startup re-attach (`svc_reattach_live_tmux_tabs.rs`, target chains in
`svc_reattach_live_tmux_tabs/target_chains.rs`) no longer leaves target chains for Resume: a
`[target, tmux]` main — known from its `.session.yaml` `target` + `tmux_session`, since a target
main records no roster entry — and every `[target, tmux, agent]` tab from the tab roster are
resolved through node 2's `tmux_location_of` as the session's recorded `owner_subject`. The main
re-attaches through the single-flight `TmuxMainReattach::attach` shared with the lazy path; a tab
re-attaches through `CliSessionManager::reattach_terminal_at_location` after probing its tmux
session on the target. Neither path relaunches an agent. `[target, tmux-attach]` tabs — a
person's own tmux — are never re-attached.

A session without `owner_subject` is left with "records no owner subject" for Resume. A target that
cannot be reached, or whose owner's vault is locked, is left naming the reason and handed to a
detach watcher in its unreachable state: `tddy-cli-sessions` gains the small public wrapper
`CliSessionManager::watch_detached` over node 1's `DetachWatch` (its start was `pub(crate)`, so no
crate outside it could hand a session to a watcher; no watcher policy changed). Startup never
blocks on a target beyond the one resolution/probe it already makes.

Every successful target re-attach restarts the session's hook-log follower: a restarted daemon's
followers died with it, unlike a dropped client's, which keep reading. The restarted follower
reads on from the offset persisted in the session's directory, delivering to the daemon's activity
handling and answering waiting hooks through the target shell.

Acceptance (`remote_target_session_acceptance::remote_target_tmux_reconnect`): a fresh daemon
re-attaches a live target main (agent not relaunched, same pane) and a live target agent tab,
leaves an unreachable target watched and re-attaches it once reachable, leaves a pre-subject
record for Resume, leaves a vault-locked target naming the vault, never attaches a person's tmux,
and reads a hook appended after a startup re-attach. The pre-existing startup test that left a
target tab for Resume now pins the gone-tmux-session reason.

Backlog delta (top node): this stack added no `docs/dev/todo/` entries that remain open here. Node
2 resolved `docs/dev/todo/2026-11-23-target-tmux-auto-reconnect.md` and narrowed
`docs/dev/todo/2026-11-22-coder-target-stop-reads-exit-code-1.md` to non-tmux target runs; both
stand. Deferred: none.
