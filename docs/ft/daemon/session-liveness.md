# Session liveness

**Status:** ✅ Shipped (`#session-liveness` 1/4 PR #958, 2/4 PR #959, 3/4 PR #960, 4/4 PR #961)

## Summary

A session is **active if and only if the daemon holds at least one live agent tab for it**: its main
agent, or any extra agent terminal tab. The pid a session's `.session.yaml` records is a record of
what was started and never decides liveness, because nothing clears it when the agent exits and the
OS reissues it to an unrelated process. Delete, `SignalSession` and the project-home
`TerminateSession` act only on processes the daemon holds.

## The rule

The daemon asks its own holders which sessions are live — the terminal manager and the sandbox
manager — and a session listing marks a session `is_active` when its id is in that set. Liveness is
per daemon: a session is active on the daemon that holds its agent, and nothing reads another host's
processes.

| Kind of session | Live while |
|---|---|
| Local PTY main (project-home, orchestrator, split agent half) | its `main` terminal resolves |
| tmux-run main | its tmux client `main` resolves, or a detach watcher holds it; after a restart, once re-attached (at startup) |
| Target (ssh / Coder) | its transport-client `main` resolves — it exits with the remote agent |
| Sandboxed | its runner process has not exited |
| Agent terminal tab (a CLI harness `kind`) | its terminal resolves |
| tmux-wrapped agent tab | its tmux client resolves, or its own detach watcher holds it, until its tmux session ends |
| Bash or tmux-attach tab | never counts — not an agent |
| `workspace` | never — it runs no agent |
| `tool` (tddy-coder) | its process tab is held, until the spawn backend that holds the child reports its exit — see [Tool sessions](#tool-sessions-as-a-process-tab) |

- A main that exited while an agent tab still runs leaves the session **active**.
- A session whose agent tabs have all closed, or that has only shell tabs, is **inactive**.
- A tmux-run agent — the main, or a tmux-wrapped agent tab — whose client ended stays live while the
  daemon's detach watcher holds its tmux session, and for the moment between the client's end and the
  watcher taking over: a listing never finds the session held by neither. Watchers are per terminal,
  so a main and each wrapped tab are watched on their own.
- A sandboxed session's entry outlives its runner until resume, relaunch or delete replaces it, so
  its liveness is read off the runner process the entry owns, never off the entry's presence or the
  recorded pid.

## A daemon restart

A restarted daemon holds nothing at first: every session lists **inactive**, whatever its
`.session.yaml` says, until it is started, resumed or re-attached.

**Startup re-attach.** Once the host is up and has bound the agent-tool sockets, the daemon reads
every session's [tab roster](#the-tab-roster) and re-attaches each `tmux` node whose tmux session is
alive, as the OS user the node records — the main and every tmux-wrapped tab — so a tmux agent that
outlived the daemon lists **active** without anyone opening it. The agent is never relaunched. A tab
whose tmux session is gone, or whose chain records no OS user, no agent or no worktree, is left as it
was, with the reason logged; Resume brings it back. A session with no roster (left by a daemon that
predates it) falls back to the main `tmux_session` its `.session.yaml` records: the OS user is the
configured user whose tmux server holds that session, and the main is left when no configured user
does. The lazy re-attach — the first stream, input or history read of a main, or Resume — stays, for
whatever the startup pass left.

Each main re-attach attempt announces how it ended — re-attached, already attached, or skipped for a
named reason — on the terminal manager's `reattach_finished()` stream and in the log under the
`tddy_cli_sessions::reattach` target.

## The tab roster

Every terminal tab a session opens is recorded in `<session dir>/tabs.yaml` as its **launch chain**:
an ordered list of nodes, outermost first, each a `kind` and a string `properties` map.

```yaml
tabs:
  - terminal_id: main
    launch:
      - kind: tmux
        properties: { session: tddy-01a1…, os_user: dev }
      - kind: agent
        properties: { harness: claude-cli }
  - terminal_id: 01a2…
    launch:
      - kind: shell
        properties: {}
```

| Node kind | Properties | Is |
|---|---|---|
| `agent` | `harness`, `conversation_id` | the agent a tab runs, and the conversation it holds (a main's conversation is `.session.yaml`'s to say) |
| `shell` | — | the session user's login shell |
| `tmux` | `session`, `os_user` | a tmux session the daemon created to wrap a tab |
| `tmux-attach` | `session`, `os_user` | a client on the user's own tmux session — attached to an existing one, or started as a new one |

- **One writer.** The roster is a file of its own, written only by the daemon's terminal roster under
  one lock per session, atomically. `.session.yaml` is read-modify-written by hook and status updates
  with no lock and refuses keys it does not know, so a roster inside it would be clobbered or would
  lock older daemons out. `.session.yaml`'s `tmux_session` stays the main's record for them.
- **Unknown content survives.** A node kind, property key, entry key or top-level key this daemon does
  not know is written back verbatim, so a later wrapper (`ssh` with `host`, …) needs no schema change
  and an older daemon never drops what a newer one wrote.
- **Lifetime.** A tab enters the roster when it starts and leaves it when it is **closed**
  (`StopTerminalSession`). A tab that exits on its own stays, so Resume can bring it back; deleting the
  session removes the file with the session directory. A tab that cannot be recorded is stopped and
  its start fails: an unrecorded tab could not be restored and nothing would know to stop it.
- **Listing.** `ListTerminalSessions` reports each recorded tab's chain as
  `TerminalSessionInfo.launch`; a terminal with no recorded chain lists an empty one.

## tmux-wrapped agent tabs

`StartTerminalSession` with an agent `kind` and `wrappers: [{kind: "tmux"}]` runs the agent in a tmux
session of its own, `tddy-<session_id>-<terminal_id>`, **as the session's OS user**, and attaches a
client tab to it (the main keeps `tddy-<session_id>`). The tab's chain is `[tmux, agent]`. Any other
wrapper kind, a second wrapper, or a tmux wrapper around anything but an agent is refused
`INVALID_ARGUMENT` naming what was refused; nothing is launched.

The agent outlives the tab's client: closing the browser tab or a daemon restart leaves it running,
and the session stays live while it does. **Closing the tab** (`StopTerminalSession`) is different from
a client ending: it ends the tab's tmux session — and its agent — before the tab, and removes the roster
entry. The web offers it as **Add agent ▸ in tmux**, and marks the tab.

## Tool sessions as a process tab

A tool session's `tddy-coder` is registered, as soon as its spawn returns, as a **process tab** of its
session: the session's `main` tab, of kind `tddy-coder`, with no PTY. It counts as a live agent, it is
not listed by `ListTerminalSessions` (the session's UI is tddy-coder's own), and it leaves when the
spawn backend reports the child's end. Each backend reports it as the daemon's own value, "exited, with
this status", never as a pid probe:

| Backend | Who holds the child | How its end reaches the daemon |
|---|---|---|
| `tddy-supervisor` | the supervisor | the daemon polls `SessionStatus` (the supervisor reaps its own children, so `EXITED` is exact) |
| forked spawn worker | the worker's wait thread | `WorkerEvent::Exited{pid, code, signal}` on the request's event stream |
| direct spawn | the spawner's wait thread | the thread sends the status on a channel |

Stopping and signalling go to the holder: `StopSession` / `SignalSession` on the supervisor, a signal
to the child the worker or direct spawn owns. The supervisor's `SignalSession` is refused for a pid it
never spawned. A resumed tool session registers its respawned child the same way, and the host-session
socket stops answering the session when the child's end is reported.

## Resume restores the roster

`ResumeSession` brings back **every tab of the session's [roster](#the-tab-roster)**, not only main, each
under its **recorded terminal id**. Main is resumed first, by its own unchanged path; then every other
entry, in roster order, is restored as its launch chain says. The restore runs on every resume path —
host, sandboxed and target sessions alike — and a main that fails still fails the resume.

| Outermost node | Restored as |
|---|---|
| a terminal already live | left alone (reported `RESTORED`) |
| `agent` (resumable) | started under its id with the harness's resume: claude `--resume <conversation id>`, cursor `--resume <chat id>` |
| `agent` (`resumable: false`) | started **fresh** under its id, reported `FRESH` |
| `tmux` → `agent` | its tmux session alive: a client is **re-attached**, the agent untouched. Gone: the agent is relaunched in a new tmux session of the same name and resumes its conversation (fresh when not resumable) |
| `shell` | the session user's login shell, fresh |
| `tmux-attach` | re-attached while the user's tmux session exists; otherwise **dropped** |

- **Resumable conversations.** A tab resumes *its own* conversation only for a harness whose conversation
  is the daemon's session id (claude) or one the agent mints (cursor). A cursor tab is now started with a
  real chat the agent creates, so it can be resumed at all; the `agent` node records `conversation_id`.
  opencode and muse `--continue` the worktree's last conversation (main's, ambiguous across tabs) and warp
  needs an exit-hook token a tab does not capture: such a tab's `agent` node carries `resumable: "false"`
  and it reopens fresh.
- **Unknown kinds are dropped.** A chain holding a node kind this daemon cannot restore — anywhere in the
  chain — is **dropped** with a reason naming the kind, never a failed resume. The same goes for an empty
  chain, a node missing a property it needs (`harness`, `session`, `os_user`, `conversation_id`), a harness
  the host no longer has, or a gone `tmux-attach` session. A dropped tab is forgotten from `tabs.yaml`.
- **Failures are per tab.** `ResumeSessionResponse.restored_tabs` has one `RestoredTab { terminal_id,
  outcome: RESTORED | FRESH | DROPPED, reason }` per restored entry; `FRESH` and `DROPPED` carry why.
- **Web.** The stopped strip is replaced by the live one with the restored tabs under their ids, and the
  tab that was active stays selected (an id the daemon no longer lists falls back to the Agent tab). A tab
  reopened fresh or dropped is named, with the daemon's reason, in a notice over the pane.

## What acts on held processes

- **Delete** stops every terminal the session has (main and tabs), its process tab through its holder,
  and its sandbox, then removes the directory. It never signals the recorded pid.
- **`SignalSession` and the project-home `TerminateSession`** signal the session's held main agent: its
  main terminal's process, its process tab's child, or its sandbox runner. With none held they are refused
  (`FAILED_PRECONDITION`, `session has no running agent`) and nothing is signalled.
- **`EnsureProjectHomeSession`** answers `running` only for a main the daemon holds; a main whose
  recorded pid is alive but whose terminal nobody serves is **resumed**.
- **The project-home idle reaper** considers only live sessions and, after stopping one, waits for it
  to leave the live set — not for a pid to exit.

## Who reads it

Every consumer of `is_active` takes the live set from the daemon's terminal and sandbox managers: the
web drawer's `ListSessions`, `ListProjectSessions` and the project soft-delete refusal, branch
ownership (an *active* owner refuses a start — see [Session branch conflict](session-branch-conflict.md)),
project-home ensure and the idle reaper, the project-home session tools and git operations, the
PR-status tools, the remote-git pack resolver (which prefers the session the daemon holds a live
agent for), and Telegram's list, branch-owner scan and delete.

## Known limitations

- A tool session's child is not re-attached after a daemon restart: the session lists **inactive**
  until it is resumed, whether or not the child outlived the daemon.
- For a forked worker or a direct spawn the daemon signals the child's pid itself. Between the kernel
  reaping the child and the wait thread's report reaching the daemon the pid could in principle be
  reissued; a signal sent in that window is not prevented. The supervisor backend has no such window.
- A tmux-run main that outlived a daemon restart is held by nothing until it is re-attached. The
  startup pass does so when the roster, or the pre-roster fallback, names an OS user the daemon can
  serve; until then a replace cannot stop it.
- What a detached tmux-run agent prints while nobody is attached is not recorded, for a main and for a
  wrapped tab alike: history serves the recording up to the client's end.
- **Only host-side tabs are restored.** Nothing starts a secondary tab inside a running jail or on a
  target today — a sandboxed or target session's tabs start host-side — so every recorded chain is host-side
  and so is its restore. Two further stack nodes are planned: **sandbox tab restore** (a jail-side tab spawn
  plus a per-tab `sandbox` wrapper node in `tabs.yaml`; sandboxed and non-sandboxed tabs can then coexist in
  one session) and **target tab restore** (a target tab launch plus a target wrapper node, restoring through
  the session's `RemoteShell`). `restore_plan` dispatches per node kind, so each adds a kind. Until then a
  chain holding such a kind is dropped with a reason, and a target session's host-recorded shell tab is
  probably dropped on resume.
- opencode, muse and warp tabs reopen fresh, not in their conversation (`TODO(opencode-cli)`,
  `TODO(muse-cli)`, `TODO(warp-cli)` on `tab_conversation_is_resumable`).
- The sandboxed resume path calls the same restore but has no test of its own: its jail harness is the
  macOS-only one, which already fails on `master`.
- A tab started as **New tmux session…** is recorded as `tmux-attach`, like one attached to an existing
  session; only the wrapper the daemon creates around an agent is a `tmux` node.

## Related

[Terminal sessions](terminal-sessions.md) · [Project-home auto-stop](project-home-auto-stop.md) ·
[Session branch conflict](session-branch-conflict.md) · [Claude CLI session](claude-cli-session.md) ·
[Inactive session activities](../web/inactive-session-activities.md) ·
[Session terminal tabs](../web/session-terminal-tabs.md#stopped-sessions)
