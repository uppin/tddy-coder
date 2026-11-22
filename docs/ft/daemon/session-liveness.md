# Session liveness

**Status:** ✅ Shipped (`#session-liveness` 1/8 PR #958, 2/8 PR #959, 3/8 PR #960, 4/8 PR #961, 5/8 PR #964, 6/8 PR #965, 7/8 PR #966)

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
| Jailed agent tab (`sandbox` around a harness) | its tab jail's runner has not exited — see [A sandboxed session's tabs](#a-sandboxed-sessions-tabs) |
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
- A sandboxed session's liveness is read off the runner process its entry owns, never off the recorded
  pid. A main jail that dies on its own leaves the registry (see
  [A dead main jail](#a-dead-main-jail)), so the entry never outlives its runner.

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
| `tmux` | `session`, `os_user`, and on a main's node `start` | a tmux session the daemon created to wrap a tab. A main's node also keeps `start`: when the agent its pane runs started, in whole seconds since the Unix epoch (see [Deleting a tmux-run session](#deleting-a-tmux-run-session)) |
| `tmux-attach` | `session`, `os_user` | a client on the user's own tmux session — attached to an existing one, or started as a new one |
| `sandbox` | — | a jail of the tab's own, around the node it wraps (see [A sandboxed session's tabs](#a-sandboxed-sessions-tabs)) |
| `target` | — | the session's target, around the node it wraps: the machine `.session.yaml`'s `target` names, in the session's worktree there (see [A target session's tabs](#a-target-sessions-tabs)). It records no properties — the transport and the worktree are the session's, read on every start |

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

## Deleting a tmux-run session

`tmux kill-session` sends the pane's process `SIGHUP` and closes its terminal, so an agent that ignores
`SIGHUP` and does not read its terminal survives it. Deleting a tmux-run session therefore ends the agent
itself, as the session's OS user:

1. The pane's pid and the time that process **started** are read (the start through `sysinfo`, so the same on
   macOS and Linux; whole seconds).
2. The tmux session is killed. One already gone is not an error and leaves nothing to signal.
3. The pid gets `SIGTERM`, then `SIGKILL` once a 5 s grace has passed if it is still there — **each only while
   the pid still names a process that started when the agent did.** A pid whose start differs is another
   process and is never signalled.

The agent's start is recorded when a tmux main starts, as the `start` property of the main's `tmux` node in
`tabs.yaml`, and that record is what the delete checks. A session whose roster kept no start (recorded by an
older daemon, or whose start could not be read) is checked against the start read just before the kill; so is
one whose recorded start is not a number of seconds, which is logged. Start times have a resolution of one
second, so a pid reissued to a process that started within the same second as the agent is not told apart.

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
never spawned.

**A signal never reaches a recycled pid.** The thread that reaps a direct or worker child first waits for it
without reaping it (`waitid` with `WNOWAIT`), then — under a lock — marks the child ended and reaps it. Every
signal is sent under the same lock, after checking the mark, so it goes out either while the pid still names
the child (alive, or a zombie nobody has reaped) or not at all: a signal to an ended child is answered "the
child has ended", and a stop of one sends no `SIGKILL` after its grace. The forked worker is the child's parent,
so the daemon asks it (`WorkerRequest::Signal`, answered `Signalled`) and the worker signals under the lock of
the child it holds; the daemon signals a direct child under its own. The supervisor signals the pid it spawned
and no other. A resumed tool session registers its respawned child the same way, and the host-session
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
| `sandbox` → `shell` | the session user's login shell, fresh, in a **new jail** of its own; dropped with the jail's reason when it cannot start |
| `sandbox` → `agent` | the agent, in a **new jail** of its own, under its recorded id: resuming its recorded conversation where the harness can (as an `agent` tab does), otherwise started fresh and reported `FRESH` with why; dropped with the jail's reason when the jail cannot start |
| `target` → `shell` | the target user's login shell, fresh, **on the session's target**, in the session's worktree there; dropped with the reason (an unreachable target, a locked vault) when it cannot start |
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

## A sandboxed session's tabs

A sandboxed session's main runs in a jail. A secondary **shell or agent** tab of that session is opened where the
person picks it, in the web's new-tab menu: **In sandbox** (the default) or **On host**. Both can coexist
in one session.

- **Jail per tab.** An *In sandbox* tab runs in a jail **of its own** — a second `tddy-sandbox-runner`
  over the session's worktree, mounted writable and the shell's working directory, lowered from the
  session's own sandbox grants exactly as its main's jail is (the same tool closure, egress policy and
  host relay). A tab's crash or exit never takes the main's jail with it, and the main's never takes a
  tab's. The cost is one runner per tab.
- **Whose shell.** The jail's runner execs the session's OS user's login shell, resolved as a host tab's is
  (`passwd`'s shell, else the serving process's `$SHELL`). The jail itself is spawned as the main's is, by
  the daemon, under the daemon's identity: there is no uid switch for either.
- **Provider route.** The grants are lowered on the session's provider route, re-resolved from the owner's
  vault as a resume re-resolves it: the route's host joins the shell's egress, and `when: unrouted` grants
  are dropped. A session whose account was removed, or whose vault is locked, is refused the tab, as it is
  refused the resume — a shell on the vendor's egress would be a wider jail than the session's own. The
  route's credentials are the agent's: the shell is given none of them.
- **Files of its own.** Each tab's jail keeps its files under `<session dir>/sandbox/tabs/<terminal_id>/`
  — IPC socket (hashed from the session and terminal ids, inside the `AF_UNIX` path limit), ready marker,
  exec plan, profile, egress log and scratch — so two tabs, and the main, never share one.
- **Records are the main's alone.** A tab jail writes no usage context, no capture recording and no
  session role: the session's usage is its agent's, its capture is its agent's terminal, and its role is
  the one its start recorded — a jailed agent tab's launch never rewrites them.
- **Recorded `[sandbox, shell]` or `[sandbox, agent]`.** The roster records the chain as a `sandbox` node
  around a `shell` or an `agent` node; a jail that cannot start fails the tab's start with why, and records
  nothing. A `sandbox` wrapper nested in `tmux` or in another `sandbox`, around anything but the login shell
  or a CLI agent kind, or on a session that is not sandboxed is refused (`INVALID_ARGUMENT` /
  `FAILED_PRECONDITION`) and nothing is launched. A tddy-coder participant, which runs bash only, refuses a
  jailed agent as it refuses a jailed shell.
- **An agent in a jail of its own.** A jailed agent tab runs the agent as the session's main does: its tool
  closure lowered into the same grants (egress, mounts, tool links), on the session's provider route, with its
  argv rendered from the agent's spec — but under the tab's own files and in **a conversation of its own**,
  minted by the rule a host agent tab's is (a fresh id, or a chat the agent mints), never the main's. A
  persistent agent home the main's jail also uses is used as it is, with no second credential bootstrap.
- **Streaming and listing.** A jailed tab streams, is listed as a shell run in a sandbox (a jailed agent tab as its agent's kind), and is served
  by its own jail through the terminal store; jail terminals keep the main's limits (no resize, no control
  lease). An *On host* tab of a sandboxed session is now reachable the same way: the store resolves any
  non-main id by its owner, a tab jail or the terminal manager.
- **Death.** A watch polls each tab jail's runner; one that exits is forgotten and its tab is unlisted. The
  tab **stays in the roster**, like a host tab that exited on its own, so Resume reopens it.
- **Teardown.** Closing the tab stops its jail; deleting the session, stopping it, and resuming it stop
  every jail of the session (Resume then opens new ones); **daemon shutdown stops every tab jail and every
  session's main jail**, which it left running before.
- **Liveness.** A shell tab's jail never counts as a live agent, like any shell. A **jailed agent tab does**: its
  session is live while that tab jail's runner runs, even when the main's has exited, and it stops counting the
  moment the runner exits or the tab is closed. Stopping or deleting the session stops its jailed agent tabs
  with the rest of its tab jails.

## A dead main jail

A sandboxed session's **main** jail is watched from the moment it is registered. The watch waits on the jail's
runner **without reaping it** (`waitid` with `WNOWAIT`, on a blocking task, holding no lock), so the exit status
is never lost to a liveness poll that asks first. A runner that exits **on its own**:

- **ends the main terminal**: its status is recorded on the jail's state and its `pty_done` fires, so a stream
  of the terminal ends where it used to hang open;
- **leaves the registry**, when it is still the jail registered for its session, and the jail's capture
  recording is finished;
- **lists the session inactive** — the live set reads the jail's runner, and it has none — while the session
  stays resumable;
- is announced once as a `JailEnded { session_id, exit }` (`SandboxSessionManager::subscribe_ended`).

A jail **stopped by request** (delete, resume, relaunch, daemon shutdown) is not announced as `JailEnded`: the stop
is marked before the kill, so the watch finds the stop already recorded. It does end the terminal's `pty_done`,
with the status the stop reaped. The session's **tab jails are untouched**: they keep their own watch, and a
jailed agent tab keeps the session live after its main's jail died.

A jail's stdio endpoint, host relay and terminal-event forwarder tasks are kept with it and aborted when it is
stopped.

## A target session's tabs

A target (ssh or Coder) session's main runs on its target, reached through its remote shell. A secondary
tab of that session — a **shell** or an **agent** — is opened where the person picks it, in the web's new-tab
menu: **On target** (the default) or **On this host**. Both can coexist in one session.

- **On target.** The tab runs `cd '<worktree>' && exec "${SHELL:-/bin/sh}" -l` on the target: the target user's
  own login shell, with the target login's own environment — a person's shell, not the `env -i` launch of an
  agent — through the session's remote shell, as a local tab terminal (one interactive `ssh -tt` or `coder ssh`
  client per tab, in the session's directory on this host).
- **An agent on the target.** An agent kind opened "On target" runs the agent on the target, in the session's
  worktree there, through the same remote shell and as the same kind of local tab terminal. The target
  resolves the agent: the tab runs its spec's **bare command** (`claude`, `agent`) found on the target
  login's own `PATH`, so a daemon host with no copy of the agent opens one. A conversation the agent mints
  itself (Cursor's chat) is minted on the target, in the worktree there; one the daemon names (Claude's
  `--session-id`) is a fresh id. A remote shell forwards no environment, so what the agent needs travels in
  its command, as a target start's does: the spec's `run.env` and the `pass_env` values the daemon holds,
  under the target login's own base (`HOME`, `PATH`, `TMPDIR`). A file the agent's argument names in the
  session's directory here is written into the worktree's `.tddy/` on the target and its argument rewritten
  to that copy, so **no path of this host reaches the target**. The agent's failure, and its exit, end only
  its tab.
- **A tab is not a main.** Opening it writes no pid file on the target, places or rewrites no session pointer,
  applies no startup window, and never stops the session: its failure, and its exit, end only the tab.
- **Whose target.** The session's `.session.yaml` `target` and `repo_path` name the machine and the worktree;
  a Coder workspace is reached with the account **the caller's** vault holds, the caller being who their
  session token resolves to. A session that records no target or no worktree, a locked vault, and an account
  the vault does not hold are refused `FAILED_PRECONDITION`, naming why.
- **Reached first.** The target is entered with `cd '<worktree>'` through the port before any terminal is
  registered, so a target that resolved but cannot be reached — its transport fails to connect, or the
  worktree cannot be entered — fails the start with the transport's own words and records nothing. A target
  that could not be *resolved* (a locked vault, a missing account, no recorded target or worktree) is
  `FAILED_PRECONDITION`: the caller can meet it.
- **Probed for its agent.** An agent tab — new or restored — then asks the target's own login shell for the
  agent's bare command, in the worktree (`cd '<wt>' && exec "${SHELL:-/bin/sh}" -lc 'command -v <command>'`),
  before any terminal is started. One extra round trip per agent tab (about 6–8 s on a Coder workspace); a shell
  tab is not probed.
- **One error for every target tab, shell or agent.** A failed launch reports:

  | What failed | Code |
  |---|---|
  | The target cannot be reached — its transport, entering the worktree, the probe's own call, or the pre-trust of a start's clone (`Trust(Shell)`) | `UNAVAILABLE`, with the transport's words |
  | This host cannot spawn the tab's local terminal | `INTERNAL` |
  | The target's login shell does not find the agent | `FAILED_PRECONDITION`, naming the command |
  | The target cannot be resolved, a malformed trust answer, anything else the launch refuses | `FAILED_PRECONDITION` |

  Nothing is recorded in any case, and no terminal is left behind.
- **Recorded `[target, shell]` or `[target, agent]`.** A `target` wrapper around anything that is neither the
  login shell nor an agent, or around an agent that wraps another wrapper, is `INVALID_ARGUMENT`. A
  tddy-coder participant, which reaches no target, refuses both (`FAILED_PRECONDITION`).
- **On this host.** A tab "On this host" of a target session is a plain shell in the session's own directory
  on this host, where its main's client runs: the recorded worktree exists only on the target. It is
  reachable whether or not the session's main runs.
- **Resume.** The roster restore reopens a `[target, shell]` or `[target, agent]` tab on the target under its
  recorded id, as the caller of the resume; it is dropped, with the reason, when the target cannot be reached,
  the vault is locked, or (an agent tab) the target lacks the agent. An agent tab resumes its conversation on the target where its harness can (claude
  `--resume <id>`, cursor `--resume <chat id>`) and reopens fresh, with the reason, where it cannot or the
  record holds no conversation.
- **Liveness.** A shell tab never counts as a live agent, on a target as anywhere. An agent tab on a target
  is an agent terminal tab like any other: it counts while its terminal — the local client of the
  transport — resolves, and that client exits with the remote agent.

## What acts on held processes

- **Delete** stops every terminal the session has (main and tabs), its process tab through its holder,
  and its sandbox, then removes the directory. It never signals the recorded pid. A tmux-run session's
  agent is ended as well, see [Deleting a tmux-run session](#deleting-a-tmux-run-session).
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
- A tmux-run main that outlived a daemon restart is held by nothing until it is re-attached. The
  startup pass does so when the roster, or the pre-roster fallback, names an OS user the daemon can
  serve; until then a replace cannot stop it.
- What a detached tmux-run agent prints while nobody is attached is not recorded, for a main and for a
  wrapped tab alike: history serves the recording up to the client's end.
- **An agent tab on a target is not restored.** An agent tab never starts on a target, so a `target` chain
  around an agent is dropped with a reason; **target agent tab** is a further stack node. `restore_plan`
  dispatches per node kind, so it adds a kind.
- **Only the tab shapes this daemon starts are restored.** `restore_plan` dispatches per node kind and a
  chain holding a kind it does not know — or a known one around something it cannot carry (a `target`
  around an agent that wraps another node) — is dropped with a reason.
- **A target's agent must be on the target's login `PATH`.** A target agent tab runs the spec's bare command and
  is probed for it first, so a target without the agent fails the tab, naming the command, instead of opening a
  terminal that exits `127`. The probe is pinned on a stand-in target only, not against a real Coder deployment
  ([todo](../../dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)). The session-**start**
  half — a start on a target still resolves the agent on the daemon's host first — is a separate decision:
  [a start on a target requires the agent on the daemon's host](../../dev/todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md).
- opencode, muse and warp tabs reopen fresh, not in their conversation (`TODO(opencode-cli)`,
  `TODO(muse-cli)`, `TODO(warp-cli)` on `tab_conversation_is_resumable`).
- The sandboxed resume path calls the same restore but has no test of its own: its jail harness is the
  macOS-only one, which already fails on `master`.
- **A jailed claude tab shares the session's hooks and loads the project's `.mcp.json`**, as the main's jail does
  ([hooks](../../dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md),
  [`.mcp.json`](../../dev/todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)).
- **The kernel sandbox of a tab jail is not tested by CI.** Every behaviour above is pinned against an
  in-memory `TabJailSpawn` over a real process; the real-jail test
  (`sandbox_shell_tab_jail_linux`) self-skips off Linux and needs a VM, which waits for VM-backed testing
  ([`2026-11-02-ci-runs-no-linux-jail`](../../dev/todo/2026-11-02-ci-runs-no-linux-jail.md)).
- **A main jail's death is watched; a restart policy is not.** A dead main jail ends its terminal and leaves the
  session inactive for the person to resume; nothing restarts it, and the workspace and claude-cli jails keep
  the per-call discovery the
  [shutdown todo](../../dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md) describes.
- **Jails run as the daemon's uid.** Running a session's jails as its OS user, through the root supervisor, is
  [recorded](../../dev/todo/2026-11-21-jails-run-as-the-daemons-uid.md); it is testable only with a VM for now.
- A tab started as **New tmux session…** is recorded as `tmux-attach`, like one attached to an existing
  session; only the wrapper the daemon creates around an agent is a `tmux` node.

## Related

[Terminal sessions](terminal-sessions.md) · [Project-home auto-stop](project-home-auto-stop.md) ·
[Session branch conflict](session-branch-conflict.md) · [Claude CLI session](claude-cli-session.md) ·
[Inactive session activities](../web/inactive-session-activities.md) ·
[Session terminal tabs](../web/session-terminal-tabs.md#stopped-sessions)
