# Tab roster and tmux-wrapped agent tabs - PRD

**Date**: 2026-11-19
**Status**: Draft
**Stack**: `#session-liveness` 2/4 — `feature/session-liveness/tmux-roster`
**Changeset**: [2026-11-19-tmux-wrapped-tab-roster.md](../../../dev/1-WIP/2026-11-19-tmux-wrapped-tab-roster.md)

## Affected Features

- [Session terminal tabs](../../web/session-terminal-tabs.md) — the tab strip and its "Add" menu
- [Terminal sessions](../terminal-sessions.md) — `StartTerminalSession`, `ListTerminalSessions`
- [Claude CLI session](../claude-cli-session.md) — tmux-run sessions surviving the daemon

## Summary

Every terminal tab a session opens is recorded in a **tab roster** kept beside the session, as a
**launch chain**: an ordered list of nodes, outermost first, each with a `kind` and a free-form
`properties` map. An agent tab can now run **wrapped in tmux** (`tmux` → `agent`), not only the
session's main. When the daemon starts, it **re-attaches every tmux-wrapped tab whose tmux session is
still alive**, main or secondary, so a surviving tmux agent is live again without anyone opening it.

## Background

Today only `main` can run in tmux. Its tmux name is derived from the session id alone, its watchers
are keyed by session, and nothing records which tabs a session had. After a restart a surviving tmux
agent stays inactive until someone opens its terminal (`#session-liveness` 1/4 made liveness come
from what the daemon holds). The developer wants a session's tabs, and how each was launched, kept so
they can be restored (4/4). The shape must also admit future wrappers, such as `ssh` wraps `tmux`
wraps `agent`.

## Proposed Changes

### What's Changing

- **Roster file.** `<session dir>/tabs.yaml`, written only by the daemon's terminal roster under one
  lock, atomically. It is a separate file so that hook and status writes to `.session.yaml`, which
  are unlocked read-modify-writes, can never clobber it.
  ```yaml
  tabs:
    - terminal_id: main
      launch:                       # outermost first
        - kind: tmux
          properties: { session: tddy-01a1…, os_user: mantasi }   # main keeps tddy-<session_id>; a tab is tddy-<session_id>-<terminal_id>
        - kind: agent
          properties: { harness: claude-cli, conversation_id: 5f3c… }
    - terminal_id: 01a2…
      launch:
        - kind: shell
          properties: {}
  ```
  - **Node kinds:** `agent` (`harness`, `conversation_id`), `shell`, `tmux` (`session`, `os_user`)
    for a tmux the daemon created to wrap a tab, and `tmux-attach` (`session`, `os_user`) for a tab
    attached to the user's own tmux session.
  - **Unknown kinds and unknown property keys are kept verbatim** across every rewrite, so a later
    wrapper (`ssh` with `host`, …) can be added without a schema change, and an older daemon never
    drops what a newer one wrote.
  - **Entry lifetime.** A tab enters the roster when it starts. Closing it (`StopTerminalSession`)
    removes it. A tab that **exits on its own** stays, so Resume (4/4) can bring it back. Deleting
    the session removes the file with the session dir.
- **tmux-wrapped agent tabs.** `StartTerminalSessionRequest` gains `repeated LaunchNode wrappers`
  (outermost first; each `{kind, map<string,string> properties}`). In this node the only wrapper
  accepted is `tmux`; any other wrapper kind is refused with `INVALID_ARGUMENT` naming it. A
  tmux-wrapped tab gets its own tmux session, `tddy-<session_id>-<terminal_id>`, and runs as the
  session's OS user, recorded on the `tmux` node.
  - `ListTerminalSessions` reports each tab's launch chain.
  - The main's tmux run is recorded in the same shape.
- **Liveness of a tmux-wrapped tab** follows the main's existing rules: live while its client is
  attached, or while its detach watcher holds it. Watchers become per terminal, not per session.
- **Eager re-attach at startup.** After boot the daemon reads every session's roster and
  re-attaches each `tmux` node whose tmux session is alive, as the recorded OS user, binding the
  agent-tool sockets first. A dead one is left for Resume (4/4). A session that existed before this
  node and has no roster falls back to its recorded main `tmux_session`, so pre-roster tmux mains
  re-attach too.
- **Web.** The tab strip's "Add agent" menu gains an **in tmux** option for each agent kind. Its tab
  shows a tmux marker.

### What's Staying the Same

- `.session.yaml` is not extended; its `tmux_session` stays as the main's record for older readers.
- Shells and `tmux-attach` tabs behave as today; they are only recorded.
- Resume still restores only main — whole-roster restore is 4/4.

## Acceptance Criteria

1. Opening an agent tab, a shell and a tmux-attach tab records each one in `tabs.yaml` with its
   launch chain, under its terminal id.
2. Closing a tab removes it from the roster; a tab that exits on its own stays in it.
3. A roster entry carrying an unknown wrapper kind or property key survives the next roster rewrite
   unchanged.
4. An agent tab started with a `tmux` wrapper runs inside its own tmux session as the session's OS
   user, and the session is live while it runs, even after main exits.
5. A tmux-wrapped tab whose client detaches stays live while its tmux session lives, and stops
   being live when that session ends.
6. A start naming a wrapper other than `tmux` is refused, naming the kind.
7. After a daemon restart, every tmux-wrapped tab (main or secondary) whose tmux session survived is
   re-attached at startup, and its session lists active without any client opening it. The agent is
   not relaunched.
8. A pre-roster tmux main (no `tabs.yaml`) is re-attached at startup from its `.session.yaml`.
9. The web "Add agent ▸ in tmux" option starts a tmux-wrapped tab and marks it in the strip.
