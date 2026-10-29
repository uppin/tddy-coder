# Project concept

**Status:** Current  
**Supersedes (connection UX):** Session selection by raw repo path alone — users now work through **Projects**.

## Summary

A **Project** is a named configuration linking a **git URL** to a **main repository path** on disk (the clone root, not a git worktree). Projects are **per OS user**, stored in `~/.tddy/projects/projects.yaml`. **Sessions** reference a **project id** in `.session.yaml` and in the `ConnectionService` API.

## Data model

| Field | Description |
|--------|----------------|
| `project_id` | UUID, assigned at creation |
| `name` | User-chosen name; also used as the directory name under the repos base |
| `git_url` | Remote URL (e.g. `https://github.com/org/repo.git`) |
| `main_repo_path` | Absolute path to the cloned repository |
| `main_branch_ref` | Optional. Remote-tracking ref (`origin/<path>`) used as the integration base for worktree fetch and checkout (e.g. `origin/main`, `origin/release/2025`). Set/updated from the Projects UI. Omitted (legacy) rows resolve live at resolution time (`origin/master` → `origin/main` → `origin/HEAD`); see [Git integration base ref](../coder/git-integration-base-ref.md). |
| `accounts` | Optional. The accounts the project uses, as provider and account id pairs, at most one per provider. Ids are minted by the credential vault of the host that stored them. Empty means no account is assigned. Set from the Projects UI; see [Account assignment](#account-assignment). |
| `local_gh` | Optional, off by default. The project's sessions take their GitHub identity from the host's own `gh` login instead of an account; see [Local gh](#local-gh). |
| `home_session_id` | Optional, per host. The session the project runs as its **main** ([project-home session](#project-home-session)) on this host, recorded by `EnsureProjectHomeSession`. Absent means main is not set up. A `home_agent` key in an older row is ignored on read and not written back. |
| `host_repo_paths` | Per-host (or per-daemon-instance) checkout paths keyed by host key; see multi-host daemon docs. |

## Account assignment

A project names the credential-vault account it uses for each provider. Different projects may use
different accounts, and several projects may share one. A project uses **one
account per provider**; assigning two for the same provider is refused, naming the provider.
Setting accounts replaces the whole list. It is written to the one host the request names; other hosts owning the same `project_id` keep their own rows (see [the open TODO](../../dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md)).

A project with no account assigned for a provider resolves to **no account**. Operations that need
one fail saying so; they never fall back to the caller's own login, to the only account in the vault,
or to an unauthenticated request. An assignment can name an account that a given host's vault does
not hold; that host reports the assignment as present but unavailable on this host, which is
distinct from not assigned. The assignment is stored by the project registry and interpreted by the
accounts service against the host's vault.

## Local gh

A project can be switched to **Local gh** from its card. Its sessions then act as the host's GitHub CLI
login rather than a vault account ([per-project GitHub identity](github-identity.md#local-gh)).

- **Setting it replaces the GitHub assignment.** Turning it on removes the project's GitHub account
  assignment in the same write; other providers' assignments stay. Turning it off leaves the project
  with no GitHub account assigned.
- **The two sources never coexist.** A project with a GitHub account assigned together with Local gh
  is refused (`InvalidArgument`), naming both and saying to turn Local gh off first.
- **The setting travels with the account list** in `SetProjectAccounts`; there is no separate request.
  Like the assignment it is written to the one host the request names, so another host owning the same
  `project_id` keeps its own row (see [the open TODO](../../dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md)).
- **Which source a session uses.** A session started with an explicit source (a start forwarded from
  another host carries the forwarding host's) uses it, and remembers it for its resumes. Otherwise it
  uses this host's row: Local gh when set, the vault assignment as before.
- **Local gh needs a readable git URL**: its host decides which `gh` login is asked, and one that
  cannot be read is refused rather than assumed to be `github.com`.

## Storage

- **Projects registry:** `~/.tddy/projects/projects.yaml` (list of projects).
- **Clone location (default):** `{home}/{repos_base_path}/{name}/` where `repos_base_path` comes from daemon config (default: `repos`).
- **Optional override:** `CreateProject` may set **`user_relative_path`** (POSIX path relative to the user’s home, e.g. `Code/my-app` or `~/Code/my-app`). When set, the clone destination is that path instead of `{repos_base}/{name}/`.

## Daemon configuration

```yaml
# Optional; default is "repos" under each user's home directory
repos_base_path: "repos"
```

## Create project behavior

1. Resolve destination: `{repos_base}/{name}/`, **unless** `user_relative_path` is non-empty — then `{home}/{user_relative_path}` (normalized; `..` and absolute paths are rejected).
2. **If that path already exists** (checked as the target OS user): **no clone** — the existing directory is registered as `main_repo_path`.
3. Otherwise: run `git clone` as the target OS user, then register the project. The clone has no deadline,
   and it lands the way a session-start clone does (below): complete at the destination, or not at all.
   `CreateProject` and `AddProjectToHost` are unary, so they report no clone progress.

## Cloning at session start

A session start on a host that does not hold the project's checkout clones it first, from the registered
`git_url` or from a peer's, into `{home}/{repos_base}/{name}/`. A clone over SSH asks the operator for a key
passphrase when the key is protected; see [cloning over SSH](clone-ssh-passphrase.md).

The clone, like every child the daemon starts as another OS user, either becomes that user or fails at
once. A daemon that is not root, asked to clone as somebody else, refuses before starting anything, with a
message naming the user it runs as, the user requested, and that a host run by `tddy-supervisor` brokers
such spawns. While a child is being started the daemon log records each step it passes — forked, groups,
gid, uid, exec — with its pid, and a step that does not finish is logged again every five seconds as
`still at <step> after <n>s`. A session-start clone that fails is logged on the host doing the clone.

**The clone is a step of the start.** A streamed start (`StreamStartSession`) reports a **clone** step
(`STEP_CLONE`) before the worktree step, and while it runs, git's own progress as `CloneProgress` events:
the stage in git's words (`Counting objects`, `Receiving objects`, `Resolving deltas`), percent, object
counts, bytes received and throughput. The events are throttled where git runs — at most four a second,
plus every stage change and every 100% — so neither the spawn worker's pipe nor a relayed stream carries
git's every redraw. A start whose project is already on the host reports no clone step. On a host run by
`tddy-supervisor` the supervisor runs git and streams its progress back (`CloneRepository`), so the
clone step looks the same, a failure carries git's own message, and the clone can be stopped.

**No deadline, and no half-written checkout.** A clone takes as long as its repository and network do;
only the operator stopping it (or the client hanging up) ends it early. Git writes into a temporary
sibling of the destination, `<dest>.partial-<uuid>`, which is renamed into place only when git succeeds.
A clone that fails or is stopped removes that directory, and a stopped clone ends git and everything it
started (ssh, index-pack) as one process group. A destination therefore either does not exist or holds a
complete clone, so a later start's "already present" check never mistakes a broken clone for a project. A
failed clone's error carries git's last lines.

## API (ConnectionService)

- `ListProjects` / `CreateProject` — manage projects.
- `StartSession` takes **`project_id`** (replaces ad-hoc `repo_path`); the daemon resolves the working directory from the project’s `main_repo_path`.
- `ListSessions` returns `project_id` per session.

## Session metadata

`tddy-core` `SessionMetadata` includes required **`project_id`**. Spawns pass **`--project-id`** to `tddy-coder`. Older `.session.yaml` files without `project_id` are skipped when listing (breaking change).

## Multi-user daemon (`tddy-daemon`)

The **`tddy-daemon`** binary is the multi-user orchestrator: serves the web bundle, exposes **AuthService** (GitHub OAuth via Connect-RPC), maps authenticated GitHub users to OS users, lists allowed tools and sessions, and spawns **`tddy-coder`** with LiveKit credentials and **`--project-id`** when applicable. **`tddy-coder --daemon`** remains for single-user local use. Service install and paths: [systemd-install.md](systemd-install.md). Connection UX: [Web terminal](../web/web-terminal.md).

## Project-home session

A project has at most one **project-home session** per host, its **main**: a standing,
**read-only**, managed-codebase `claude-cli` session on the project's home worktree, pinned first in
the project's group in the session drawer
([Session drawer § Pinned main entry](../web/session-drawer.md#pinned-main-entry)). The project
records which session it is as `home_session_id` on that host's row; `ListProjects` reports it as
`ProjectEntry.home_session_id` (empty = main is not set up). The session is found by that id, never
by scanning for the role.

`SessionService.EnsureProjectHomeSession(project_id, daemon_instance_id, start?)` owns its lifecycle
and answers `{ session, outcome }`. Without `start` it reaches the recorded main:

| What the project records on that host | Outcome |
|---|---|
| No main | `NOT_FOUND` ("project X has no main session: set one up"); nothing starts |
| A main whose session directory no longer exists | the record is cleared, then `NOT_FOUND` as above |
| A main whose session record cannot be read | `INTERNAL`; the record is kept |
| A stopped main | the home worktree is fast-forwarded to the fetched main branch ref, then the session is resumed — same conversation → `resumed` |
| A running main (its pid is alive, or the daemon still serves its terminal) | returned unchanged → `running` |

With `start` (a `StartSessionRequest`, built by the New-session form's
[main variant](../web/session-drawer.md#main-variant)) it sets main up, or replaces it:

1. **The spec is checked; nothing starts or stops when it is refused.**
   - A `session_role` → `PERMISSION_DENIED`: the role is granted by the daemon, never requested.
   - A placement other than the home worktree on this host — `sandbox`, `sandboxed_codebase`,
     `codebase_daemon_instance_id`, `ssh_config_host`, `agent_clone`, `split_agent` →
     `INVALID_ARGUMENT` naming the field. A sandboxed main is refused because the sandboxed
     `claude-cli` start does not carry the session's role.
   - The harness profile named by `agent` is applied, then any session type but `claude-cli` →
     `FAILED_PRECONDITION` naming the type (`tool` for an empty one) and that main could not be kept
     read-only. Only `claude-cli` can enforce managed-codebase mode: Cursor CLI has no tool
     allowlist, and Tool, OpenCode and Muse Code sessions have no managed mode. A profile cannot
     swap a type in after this check.
2. **The daemon places the spec.** It sets `project_id`, `repo_path` (the home worktree, created at
   the main branch ref when absent), `managed_codebase = true` and `session_role = project-home`,
   and clears the branch fields (`branch_worktree_intent`, `selected_branch_to_work_on`,
   `new_branch_name`, `selected_integration_base_ref`, `create_remote_branch`,
   `on_branch_conflict`), the stack fields and `daemon_instance_id`. Everything else the spec chose
   is kept: model, provider account, `provider_built_in`, agent, permission mode,
   `dangerously_skip_permissions`, initial prompt, specialized agents, semantic index.
3. **The start is checked as the start path checks it** (model, recipe, provider route), so a
   refused replace leaves the running main running and recorded.
4. **A recorded main is made way for.** The home worktree is fast-forwarded, and a recorded main
   this daemon serves is stopped. It keeps its record and role, so it stays read-only, and is listed
   as an ordinary session from then on. A main whose process outlived a daemon restart is not
   served, so it is not stopped.
5. **Main starts and is recorded** → `replaced` when a main was recorded, `started` otherwise
   (including after a stale record was cleared). A start that fails past the checks (the agent does
   not spawn) leaves the old main stopped but still recorded, so an ensure without `start` resumes
   it. A started main whose id cannot be recorded is stopped again and the error returned.

A main that existed before the project recorded one (found only by its role) is not adopted: it is
an ordinary, still read-only session, and main is set up once more.

- **Role.** The session's `.session.yaml` carries `session_role: project-home`, and `ListSessions`
  reports it as `SessionEntry.session_role` (empty for every other session). The role is written into
  the session's initial record **before its agent is spawned**, so no moment exists in which the agent
  runs and the daemon reads the session as ordinary. Only the daemon grants it: a `StartSession` or
  `StreamStartSession` that sets `StartSessionRequest.session_role` is refused with
  `PERMISSION_DENIED` and starts nothing.
- **Home worktree.** `<worktrees root>/<project_id>/home`, a linked worktree of the project's main
  repository, **detached** at `main_branch_ref` after fetching it. It is isolated from whatever
  branch the main checkout has, has no commits of its own, and is only ever fast-forwarded. A path
  there that is not that worktree is refused.
- **Read-only.** The daemon's `ExecuteTool` and `StreamExecuteTool` refuse, with
  `PERMISSION_DENIED`, every exec tool but `Read`, `Grep`, `Glob`, `ReadLints`, `SemanticSearch` and
  the four [project-home tools](#project-home-tools) for a project-home session, so the guarantee does
  not rest on the agent's allowlist. Claude's native filesystem and shell tools (`Read`, `Write`,
  `Edit`, `MultiEdit`, `NotebookEdit`, `Bash`, `BashOutput`, `KillShell`, `Glob`, `Grep`, `LS`) never
  cross that route, so the claude-cli main is launched with every one of them in `--disallowedTools`:
  its only reach is the `tddy-tools` set the daemon admits.
- **What the agent is offered.** `ExecToolService.ListExecTools` takes a `session_id` and answers the
  set that session's agent is offered, read from the session's own record: the read-only tools plus
  the project-home tools for a project-home session, the catalog without the project-home tools for
  any other. `tddy-tools` asks it on every `tools/list` over every transport (the daemon's HTTP relay
  and unix socket, the sandbox socket — the runner rebinds the question to the jail's own session —
  and LiveKit) and advertises only what the daemon admits. A failed or timed-out ask (10 s) fails
  `tools/list` with an error naming the reason, rather than offering a catalog of its own. The
  co-located claude-cli main is also started (and resumed) with one `--allowedTools` per tool of
  that set, which pre-approves them; it does not restrict — enforcement stays the daemon's.
- **How the main reaches its tools.** The claude-cli main is launched (and resumed) with
  `tddy-tools --mcp` — the deployment's toolchain binary — as its **only** MCP server
  (`--mcp-config <session dir>/claude-mcp-config.json` and `--strict-mcp-config`). Its process
  environment names the main itself (`TDDY_REMOTE_SESSION_ID`) under a credential of its own
  (`TDDY_REMOTE_SESSION_TOKEN`): minted by this daemon from the **verified** caller of the ensure or
  resume, with the 24 h agent lifetime, never the caller's own token; a resume mints afresh. On an
  embedded daemon (Tddy Desktop) `TDDY_REMOTE_DAEMON_SOCKET` names its agent-tool socket, which
  `tddy-tools` prefers; on a binary `tddy-daemon` the tools reach `TDDY_REMOTE_DAEMON_URL`, its local
  HTTP URL. Because the main's record holds the home worktree, the daemon runs each call there, under
  the role guard above. A start or resume that cannot mint the credential, or cannot find the
  toolchain's `tddy-tools`, is refused and starts nothing. The main's MCP config and role prompt are
  written into its session directory only when its agent is spawned, so a refused start leaves
  nothing on disk.
- **Role prompt.** The main is launched with `--append-system-prompt-file` naming a short prompt in
  its session directory: it is the project's main session on a read-only home worktree, it has no
  shell and cannot edit files, and it sees and starts the project's sessions with
  `ListProjectSessions` / `StartSession`, inspects them with `ReadSessionWorktree` /
  `DiffSessionWorktree`, and manages their worktrees with the [git tools](#git-tools); to change code
  it starts a session.
- **Hosts.** A request whose `daemon_instance_id` names another daemon is forwarded to it (as
  `GetWorktreeSnapshot` is); empty or the local instance is answered locally. Each host has its own
  home session for a project.
- **Serialized.** Ensures are serialized on a daemon, and each reads the project's record only once
  it holds its turn, so two concurrent set-ups record one main: one answers `started`, the other
  `replaced`. Deleting main is allowed; the next ensure answers `NOT_FOUND` and clears the record.
  Because main is managed, it opens with the activity sidebar.
- The "running" decision is a heuristic with one known false positive: a live pid with no served
  terminal reads as running.

### Project-home tools

The exec tools below give the read-only main its reach over **its own project's** sessions. Only a
session whose record carries the `project-home` role may run them; any other caller is refused with
`PERMISSION_DENIED` naming the role — except that a session that can spawn conversations is offered
and served the four start tools and the two answer tools, for its own project (see
[Conversation spawners](#conversation-spawners)).

| Tool | Arguments | Answers |
|---|---|---|
| `ListProjectSessions` | — | `{"sessions": [{session_id, branch, worktree, is_active, agent_status, pending_elicitation, daemon_instance_id}], "unreachable_hosts": [{daemon_instance_id, reason}]}`: every other session of the project **on every eligible host**, each with the host it runs on, and the status the web's session list shows. A host that does not answer is named in `unreachable_hosts`, never silently left out |
| `StartSession` | the `StartSessionRequest` fields the New-session form can send, under their proto names: `session_type` (required), `daemon_instance_id`, `tool_path`, `agent`, `recipe`, `model`, `provider_account_id`, `provider_built_in`, `branch_worktree_intent`, `new_branch_name`, `selected_integration_base_ref`, `selected_branch_to_work_on`, `create_remote_branch`, `initial_prompt`, `permission_mode`, `dangerously_skip_permissions`, `stack_parent`, `stack_parent_daemon_instance_id`, `stack_node_id`, `pr_stack_base_session_id`, `sandbox`, `managed_codebase`, `sandboxed_codebase`, `codebase_daemon_instance_id`, `ssh_config_host`, `semantic_index`, `specialized_agents`, `attachments` (host documents) | `{"start_id", "task_id", "daemon_instance_id"}` at once (see [Asynchronous session start](#asynchronous-session-start)); the start's outcome is a session started in the project as the caller's user, on this host or the eligible host `daemon_instance_id` names; the daemon that creates it records the home session in its changeset as `orchestrator_session_id`, with the home session's host as `orchestrator_daemon_instance_id`. See [StartSession](#startsession) |
| `DescribeSessionStartOptions` | `daemon_instance_id?`, `session_type?`, `agent?`, `provider_account_id?` | the choices a `StartSession` on that host can make, from the RPCs the New-session form fills its pickers from. See [Start options](#start-options) |
| `ReadSessionWorktree` | `session_id`, `path` (relative; empty for the root), `daemon_instance_id?` | a file's text (`content`, `truncated`, `byte_size`) or a directory's `entries`, through the same worktree reader as the Code pane |
| `DiffSessionWorktree` | `session_id`, `daemon_instance_id?` | `{session_id, base_ref, diff}`: the worktree's diff against its integration base (the session's recorded base, else the project's main branch ref), including uncommitted and untracked files. Taken over a temporary index; the worktree's own index is never written |
| `ListWorktrees` | `cursor?`, `limit?` (50 by default, at most 200), `daemon_instance_id?` | `{worktrees, total, next_cursor, has_more, clamped_to?, sizing: {done, total}}`: the project's worktrees on that host as the **Worktrees screen** shows them — each row carries the screen's `WorktreeRow` fields (`disk_bytes` absent when never sized) — in the screen's order: largest first, never-sized last, then by name. Pages resume after the last row by sort position, so a size that changes between pages neither repeats nor skips a row; a cursor the tool did not issue is refused, and a larger `limit` is clamped with `clamped_to` saying so. `sizing` counts the rows not being sized, of all of them. Starts no size calculation |
| `GetWorktree` | `path` (as `ListWorktrees` lists it), `daemon_instance_id?` | `{worktree, session_id}`: that row, and the session on that host whose worktree it is (`null` when none). A path that is not one of the project's worktrees is refused naming it |
| `ReadSessionActivity` | `session_id`, `daemon_instance_id?`, `cursor?`, `limit?`, `statuses?` (`running`, `completed`, `error`; all when empty) | `{items, next_cursor, has_more, clamped_to?, pending_question}`: the session's tool calls (`agent-activity.jsonl` records, each with its `activity_seq`), oldest first, after `cursor`, and the question it waits on, or `null`. See [Following a session](#following-a-session) |
| `ReadSessionTranscript` | `session_id`, `daemon_instance_id?`, `cursor?`, `limit?`, `direction?` (`newest_first` (default) or `from_cursor`), `tool_call_id?` | `{items: [{seq, kind, text, tool_call_id, truncated}], next_cursor, has_more, clamped_to?}`: the frames the web's Activities view pages (`GetAcpReplayPage`), each text cut at 4096 bytes and marked `truncated`; with `tool_call_id`, `{tool_call_id, raw_input, raw_output}` whole instead (`GetAcpToolCallDetail`). See [Following a session](#following-a-session) |
| `TerminateSession` | `session_id`, `daemon_instance_id?` | `{session_id, signal: "SIGTERM"}`: what the web UI's **Terminate** does — `SignalSession` with SIGTERM to the session's process on its host. The session's record, worktree and branch are kept, so it can be resumed; deleting them is the UI's separate Delete, which is not offered. A session whose process has already ended (or that records none) is refused as an `is_error` result saying so. See [Terminating a session](#terminating-a-session) |
| `RefreshWorktreeDiskUsage` | `path?` (as `ListWorktrees` lists it; every worktree when empty), `daemon_instance_id?` | `{queued}`: what the Worktrees screen's **Refresh** does — queues the background size walk (`WorktreeSizeCalculator`, two walks at a time) for one worktree or all of the project's, and answers at once. Progress is read with `ListWorktrees` (`sizing {done, total}`, the screen's "{done} of {total}") or `GetWorktree` (`size_status`: `calculating` until cached). A path not in git's worktree list is refused naming it. No ledger of its own: the calculator's persisted per-path state is the record |
| `ReadSessionTerminal` | `session_id`, `daemon_instance_id?` | `{lines, cursor: {row, col, visible}, cols, rows, offset}`: what the session's **main** terminal shows right now, as text — the visible rows of the screen it is on (the alternate screen while a full-screen TUI runs), each with trailing whitespace trimmed, and the cursor, zero-based. `offset` is the position in the terminal's output stream the text corresponds to: the same sequence point a reconnecting browser's snapshot carries. Text only: no scrollback, colours or attributes. Reading sends no input, takes no control lease and disturbs no attached screen. A session with no live terminal, or whose terminal has not reported its size, is refused naming why. See [Reading a terminal](#reading-a-terminal) |
| `ClaimSessionTerminal` | `session_id`, `daemon_instance_id?` | `{holder}`: takes the session's terminal control lease for the screen `project-home:<home session id>`, stealing it from whoever holds it, as a browser's claim with `steal` does — the change is broadcast, so an attached browser drops to watch-only and names the new holder. The control token stays on the host that owns the terminal and is never returned. There is no release: a person takes control back by claiming it in the browser. See [Typing into a terminal](#typing-into-a-terminal) |
| `SendSessionKeys` | `session_id`, `daemon_instance_id?`, `text?`, `keys?` | `{sent_bytes}`: types `text`, then each named key in order, into the session's **main** terminal while this home session holds its lease. Refused, naming the holder, when it does not, and nothing is sent. See [Typing into a terminal](#typing-into-a-terminal) |
| `AnswerSessionPermission` | `session_id`, `request_id`, `option_id` or `cancelled`, `message?`, `daemon_instance_id?` | `{session_id, request_id, answered_by, option_id, cancelled, at_unix_ms}`: answers the Claude permission prompt a session this caller started is waiting on, as `ReadSessionActivity`'s `pending_question` shows it. `option_id` is `allow_once`, `reject_once` (with `message` saying why), or `allow_always` when Claude suggested a rule; `cancelled` leaves the prompt to the person at its terminal. Only the target's orchestrator may answer, and the first answer wins. See [Answering a permission prompt](#answering-a-permission-prompt) |
| `AnswerSessionQuestion` | `session_id`, `request_id`, `answers: [{question_index, selected, other?}]`, `daemon_instance_id?` | `{session_id, request_id, answered_by, answers, at_unix_ms}`: answers the `AskUserQuestion` a session this caller started is waiting on, as `ReadSessionActivity`'s `pending_question` shows it. Each answer names a question by index, the option labels chosen and, optionally, free text. Only the target's orchestrator may answer, the answers are checked against the question as asked, and the first answer wins. See [Answering a question](#answering-a-question) |
| `WaitProjectNotifications` | `cursor?`, `timeout_ms?` (at most 20000, the default), `daemon_instance_id?` | `{items, next_cursor, has_more, timed_out, lost?}`: returns at once when the project's notification log holds records after `cursor`, otherwise holds until one arrives or `timeout_ms` passes. Without a cursor it waits for the next new record. See [Waiting for notifications](#waiting-for-notifications) |
| `ReadProjectNotifications` | `cursor?`, `limit?` (50 by default, at most 200), `daemon_instance_id?` | `{items, next_cursor, has_more, clamped_to?, lost?}`: up to `limit` records after `cursor`, oldest first, without waiting; without a cursor, the newest page. See [Waiting for notifications](#waiting-for-notifications) |

- **Targets.** A target is a session id in the caller's project, with a worktree, on the host the
  optional `daemon_instance_id` argument names (as `ListProjectSessions` reports it; empty or absent
  is the caller's own host). Another project's session, the home session itself, an unknown id and a session without a
  worktree are refused as an `is_error` result naming why ("another project", …).
- **Paths.** An absolute path, one climbing above the worktree, or one resolving through a symlink to
  outside it is refused as an `is_error` result naming "outside the worktree". A base ref that git
  would read as an option is refused too.
- Every call is appended to the home session's tool-call log, as any exec tool is.
- **Worktrees.** `ListWorktrees` and `GetWorktree` read the same snapshot the Worktrees screen's
  `StreamWorktreeStats` opens with, from the daemon's one size calculator, so the tool and the screen
  cannot disagree. Clear, Delete and Restore are not offered
  ([TODO](../../dev/todo/2026-10-25-project-home-worktree-clear-delete-restore.md)). **Known
  limitation:** the name tie-break mirrors the browser's `localeCompare` for ASCII names (case-insensitive,
  lowercase first on a case-only difference, punctuation before digits before letters); for non-ASCII
  names it is an approximation and may order equal-sized worktrees differently from the screen.
- **Across hosts.** A target tool call (read, diff, the two worktree tools, `DescribeSessionStartOptions`, the two follow tools, `TerminateSession`, `RefreshWorktreeDiskUsage`, `ReadSessionTerminal`, `ClaimSessionTerminal`, `SendSessionKeys`, `AnswerSessionPermission`, the two notification tools, every git tool) naming another host is
  forwarded whole to that host over the peer forward and runs there unchanged: that host's path
  containment, activity gate and push identity apply. The host receiving it does not hold the calling
  session, so it confirms the caller **at the daemon that owns it**: it asks every other eligible
  daemon `ListSessions` with the caller's token and serves the call only when the owner reports the
  session with the `project-home` role, taking the project from that row. An ordinary session that
  addresses a request at another host itself is refused there with `PERMISSION_DENIED`. A forwarded
  call is served for that host only — its listing does not fan out and it cannot name a third host —
  so a call travels at most one hop.

### StartSession

`StartSession` offers what a person can choose in the web UI's New-session form, so an orchestrator
can start any session a person can.

- **Arguments are the request's own fields.** Every `StartSessionRequest` field is exactly one of: an
  argument, a field the caller's context fills (`session_token`, `project_id`, `on_branch_conflict`,
  `orchestrator_session_id`, `orchestrator_daemon_instance_id`), or a field the tool withholds with a
  stated reason (`repo_path`, `claude_args`, `requested_session_id`, `agent_clone`, `split_agent`,
  `github_source`, `session_role`). An argument the tool does not offer — a role among them — is
  refused by name. Two parity tests hold this: one against the compiled-in proto descriptor, one
  against every key the form's request builder can send.
- **The schema states the closed choices.** `session_type`, `permission_mode`,
  `branch_worktree_intent` and an attachment's `scope` are enums; `daemon_instance_id` lists `""`
  (this host) and every eligible host, and `recipe` every workflow recipe the daemon supports, as the
  daemon answers `ListExecTools`. `if`/`then` blocks make `model` required for the harnesses that need
  one and `selected_branch_to_work_on` required with `work_on_selected_branch`.
- **A branch another session owns is refused**, naming the branch and its owner; it is never
  suffixed (`on_branch_conflict` is always `reject`).
- **Across hosts.** A start naming another host takes the daemon's own start path, which forwards it
  there; that host creates the session and records the orchestrator in it.
- **Known limitations.**
  - A `tool` session (or an empty session type) started with an orchestrator is refused with
    `FAILED_PRECONDITION`: its changeset is written by its own `tddy-coder` process, which does not
    record one.
  - The agent's client registers its MCP tools before it asks the daemon, so the schema it sees names
    hosts and recipes as free strings; the daemon refuses a host it cannot route to and a recipe it
    does not support.
  - The orchestrator is written into the changeset after the start returns, not in the changeset the
    session type first writes.

### Asynchronous session start

`StartSession` returns **at once** with a handle, `{"start_id", "task_id", "daemon_instance_id"}`,
instead of waiting for the clone, the worktree and the agent. It opens the streamed start
(`StreamStartSession`, forwarded to the host the arguments name), waits for the start to name its
task (`StartTaskStarted`), and returns. A start refused before it begins (a branch another session
owns, an invalid option) is the tool's `is_error` result naming why, not a handle.

| Tool | Arguments | Answers |
|---|---|---|
| `GetSessionStart` | `start_id` | the start's record: `state` (`running`, `succeeded`, `failed`, `cancelled`), its current `step` `{step, status}`, `clone_progress` `{stage, percent}`, and once it succeeded its `session_id`, `branch` and `worktree` (as the host running it lists them), or once it failed or was cancelled, `error` |
| `CancelSessionStart` | `start_id` | returns at once; the start's host is sent `CancelTask` (locally, or forwarded to the peer running it), and the record reads `cancelled` once the stream has ended. A start that is no longer running is refused naming its state |

- **The start ledger.** A background consumer on the orchestrator's host drains the start's stream
  into a record at `<home session dir>/artifacts/session-starts/<start_id>.json`
  (`packages/tddy-tool-engine/src/start_ledger.rs`). The record outlives the task registry's
  five-minute retention, and a start forwarded to another host is followed the same way, since the
  forwarded stream relays the peer's events.
- **A restart.** The first time a daemon loads a home session's ledger, a record still `running` —
  one an earlier daemon was draining when it stopped — is marked `failed: daemon restarted during
  the start`.
- Tool, sandboxed and stack-child starts emit no steps, so their record reads `running` with no
  step until the outcome
  (`docs/dev/todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md`).
- A cancelled start removes what it created, including the branch it cut; see
  [Background tasks § Session starts run as tasks](background-tasks.md#session-starts-run-as-tasks).

### Conversation spawners

A session whose recipe gives it the conversation-spawn capability (today `grill-me`) is an
orchestrator too, limited to starting sessions:

- **Offered the four start tools and the two answers.** `StartSession`, `GetSessionStart`,
  `CancelSessionStart`, `DescribeSessionStartOptions`, `AnswerSessionPermission` and `AnswerSessionQuestion` are added to its `ListExecTools` set (`SessionToolRole::ConversationSpawner`)
  and served to it, with `project_id` fixed to its own project and itself as the orchestrator. The
  other project-home tools are still refused to it; an ordinary session is offered none of them.
- **`spawn_conversation` is the same start.** The agent's `spawn_conversation` tool (MCP and
  `tddy-tools spawn-conversation`) takes `StartSession`'s arguments and runs the very start
  `StartSession` runs: it begins on the streamed start path (forwarded to the host the arguments
  name), is recorded in the orchestrator's own start ledger, and answers at once with
  `{"start_id", "task_id", "daemon_instance_id"}`. `spawn_conversation_status` /
  `tddy-tools spawn-conversation-status` read the ledger record (`GetSessionStart`'s answer) and
  `spawn_conversation_cancel` / `tddy-tools spawn-conversation-cancel` cancel it
  (`CancelSessionStart`'s). A start the form would refuse (a CLI session without a model, a branch
  another session owns) is refused in place of a handle.
- **Authentication.** The start authenticates as the orchestrator's own session token — the one its
  start recorded — so a start on another host and its cancel carry it. A session holding no token
  starts nothing.
- **No defaults inherited.** A `claude-cli` conversation names its own `model`; the orchestrator link
  is `orchestrator_session_id`, not `stack_parent`.
- **Known limitation.** A spawner's tool call forwarded from a host that does not hold the session is
  refused: the owning daemon's `ListSessions` row carries the role, not the recipe, so the capability
  cannot be confirmed there
  ([TODO](../../dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md)).

### Start options

Several `StartSession` choices depend on the host and on one another, and the form fetches them live.
A schema frozen when the agent registers its tools would go stale, so `DescribeSessionStartOptions`
answers them on demand, each from the RPC the form calls, on the host `daemon_instance_id` names
(forwarded whole to another host, as the target tools are):

| Field | Source (as the form reads it) |
|---|---|
| `daemon_instance_id` | the host that answered |
| `agents` `[{id, label}]` | `CatalogService.ListAgents` |
| `models` `{default, choices: [{id, label}]}` | `AccountsService.ListProviderAccountModels` when `provider_account_id` is named (the first preselected); else `CatalogService.ListAgentModels` for `agent`, or for `session_type` when it is a CLI harness |
| `provider_routes` `[{account_id, label, preset, supported, reason}]` | `AccountsService.ListProviderRoutes` for the harness: the `session_type`, or the `agent` of a `tool` session; `warp-cli` has none |
| `ssh_config_hosts` | `HostService.ListSshConfigHosts` |
| `subagents` | `CatalogService.ListSubagents`, each qualified `name@daemon_instance_id` as `specialized_agents` takes them |
| `branches` | `ProjectService.ListProjectBranches` for the project's checkout on that host |
| `stack_parents` | the host's `ListSessions`, filtered as the form's `prStackOrchestrators`: a PR-stack recipe, not itself a stack child |
| `pr_stack_base_candidates` | the same sessions, filtered as the form's `stackBaseSessionCandidates`: owning a branch, in this project, on this host, not already stacked |

- **An unanswerable list says why.** `models`, `provider_routes` and `ssh_config_hosts` that could not
  be answered are empty, with `models_unavailable`, `provider_routes_unavailable` or
  `ssh_config_hosts_unavailable` naming the reason: the query names no agent or harness, the caller's
  vault refused, the host could not list its SSH config, or **the host keeps no credential vaults**
  and so has no provider accounts to offer. A failure of any other source (the catalogue, the branch
  listing, the session store) fails the whole answer.
- **The provider accounts are the host's own.** The daemon serves the tool from the same accounts
  service instance its Accounts screen RPC serves, attached when the host has credential vaults, and
  each read is the caller's own vault.
- **Stack parents are not limited to the project**, exactly as in the form: a PR-stack orchestrator
  of another project is offered. The daemon's own stack-parent checks still apply at start.
- **Parity.** The two stack filters are mirrored in Rust
  (`packages/tddy-tool-engine/src/start_options.rs`) and run, with the form's own TypeScript
  functions, over one shared fixture
  (`packages/tddy-web/cypress/support/fixtures/stack-parent-candidates.json`).

### Reading a terminal

`ReadSessionTerminal` answers from the daemon's screen model of the target's `main` terminal
(`tddy_task::TerminalCapture::text_grid`), the model a reconnecting browser is repainted from, so the
agent and the screen cannot disagree.

- **Which screen.** The buffer in use: the alternate screen while an application is full-screen, the
  primary screen otherwise. Scrollback is not kept by the model.
- **Which terminal.** The one the terminal RPCs resolve for `main` — the sandbox's first, then the
  host's. A jailed terminal is readable once the jail has reported its size.
- **One moment.** Rows, cursor, size and `offset` are read under the capture's lock. Rows are trimmed
  at the end, so a blank row is an empty string.
- **Refusals** are `is_error` results: `no live terminal` (with the session id) and `terminal size is
  not known yet`, besides the target refusals every target tool shares.

### Typing into a terminal

`ClaimSessionTerminal` and `SendSessionKeys` act through the same single-screen control lease the
browsers share ([Terminal sessions § Terminal Control Ownership](terminal-sessions.md#terminal-control-ownership-single-screen-mutex)).
The rule is "type only while you hold it", since leases are never released.

- **Whose lease.** The home agent holds it as the screen `project-home:<home session id>`. The token
  is kept on the host that owns the terminal, keyed by the calling home session, the daemon that owns
  that session, and the target, so two homes never share one — on one host or across two.
- **Every send is checked.** The holder must be this home's screen, this home must have a remembered
  token, and that token must verify against the lease. The check is made explicitly, so a jailed
  terminal — whose own input path does not consult the lease — follows the same rule. A refusal names
  the holder (or says nothing holds control) and says to claim again; nothing is typed.
- **Text, then keys.** `text` is typed first: as one bracketed paste carried verbatim when the
  terminal has enabled mode 2004, otherwise raw, each line break typed as Enter (`\r`). Text holding
  the paste-end sequence `ESC[201~` is refused while pasting, since it would end the paste early.
  Typing text reads the terminal's paste mode first, so a terminal that has not reported its size
  refuses `text` (keys alone still go). `Enter` is never implied.
- **Key names**, case-sensitive: `Enter` `\r`, `Escape`, `Tab`, `BTab` `ESC[Z`, `Backspace` `0x7f`,
  `Delete` `ESC[3~`, `Up`/`Down`/`Right`/`Left` `ESC[A..D`, `Home` `ESC[H`, `End` `ESC[F`,
  `PageUp`/`PageDown` `ESC[5~`/`ESC[6~`, `Space`, `C-a` … `C-z` (`0x01` … `0x1a`), `F1`–`F4`
  `ESC O P..S`, `F5`–`F12` `ESC[15~` … `ESC[24~`. An unknown name refuses the whole request before
  any byte is written; so does a request with neither text nor keys.
- **Input offset.** Input is written with offset 0, which the terminal treats as unset and does not
  acknowledge, so a browser's un-acknowledged-input overlay is untouched.
- Not offered: a release tool, mouse input, resizing, or typing into the home session itself.

### Answering a permission prompt

A claude-cli session started by an orchestrator (its changeset's `orchestrator_session_id`) holds
its `PermissionRequest` hook open for that orchestrator's decision, for at most
`claude_cli.answer_wait_secs` ([Claude Code CLI session § Waiting for an orchestrator's answer](claude-cli-session.md#waiting-for-an-orchestrators-answer)).
`AnswerSessionPermission` gives that decision.

- **The request** is offered in the same shape as an ACP subagent's (`PermissionRequestView`): the
  tool as `title`, its ACP `kind` (`execute` for `Bash`; `edit` for `Edit`, `MultiEdit`, `Write`,
  `NotebookEdit`; `read` for `Read`, `Glob`, `Grep`; `other` otherwise), its parsed input, and the
  options `allow_once` ("Allow once"), `allow_always` ("Allow always", only when Claude sent
  permission suggestions) and `reject_once` ("Reject").
- **Checks, in order**, each refused as an `is_error` result naming why, and each leaving the hook
  waiting:
  1. the caller is the target's orchestrator — the session `orchestrator_session_id` names, on the
     host `orchestrator_daemon_instance_id` names (the target's own host when absent): `permission
     denied: session <caller> is not the orchestrator of session <target> (its orchestrator is …)`;
  2. nobody answered it already: `permission request '<id>' was already answered by orchestrator
     session <id>` (the daemon remembers its last 1024 answers);
  3. the target's pending question is open under that `request_id`, and a hook is still waiting on
     it: `no permission request '<id>' is pending on that session` — also after the wait timed out;
  4. it is a permission, not a question: `… is a <kind> question, not a permission: answer it with
     AnswerSessionQuestion`;
  5. the option is one the request offers: `permission request '<id>' offers no option '<option>'`.
- **Then** the hook is woken with the answer, a `completed` row under `permission:<request_id>` is
  appended to the target's activity (the request, and who answered what), and its pending question
  is cleared. Once the hook is woken the answer stands: failing to write the row or clear the
  question is logged, and the child's next status report clears the question.
- **Same host only.** The hook waits only for an orchestrator whose main terminal its own daemon
  serves. An orchestrator on another host gets no wait, and Claude's own prompt appears
  ([TODO](../../dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md)).
- **Known gap.** A conversation spawner may answer but is not served `ReadSessionActivity`, so it
  cannot read the `request_id`; its claude-cli children wait out the deadline
  ([TODO](../../dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md)).

### Answering a question

An orchestrated claude-cli session's `AskUserQuestion` waits the same bounded way a permission does
(`PreToolUse` hook, [Claude Code CLI session § Waiting for an orchestrator's answer](claude-cli-session.md#waiting-for-an-orchestrators-answer)),
and `AnswerSessionQuestion` gives the answers. With no orchestrator on the host, or no answer in
time, the child shows its own dialog.

- **Checks, in order**, each refused as an `is_error` result naming why, and each leaving the hook
  waiting: the caller is the target's orchestrator; nobody answered it already; the target's pending
  question carries a `request_id` (a `tool` session's clarification does not: it is answered through
  that session's own presenter) and is the one asked for; it is a question, not a permission (`…
  answer it with AnswerSessionPermission`); a hook is still waiting on it; and the answers fit the
  question the waiter recorded.
- **What fits.** `other` is free text and always accepted. Refused, naming the question: an index
  the call has no question at; a question answered twice; a label the question does not offer, as
  spelled; more than one answer to a single-choice question (a label and `other` count one each);
  an answer that selects nothing. A question the answers leave out is not refused: Claude shows it.
- **Then** the hook is woken with the answers, a `completed` row under `question:<request_id>` is
  appended to the target's activity (`{"answeredBy", "answers": [{"questionIndex", "selected",
  "other"?}]}`), and the target's pending question is cleared if it is still that question. As for a
  permission, once the hook is woken the answer stands: a failed write is logged, not refused.
- **Same host only, and the spawner gap**, as for a permission.

### Waiting for notifications

`WaitProjectNotifications` and `ReadProjectNotifications` tell the project-home agent what happened in
its project without polling: a session finished, waits for input, asks a question or a permission, or
an agent wrote it a notification.

- **The log.** Every `ATTENTION_REQUIRED` notification of a project's sessions
  ([Session notifications](session-notifications.md#the-project-notification-log)) is appended to
  that project's log on the daemon that published it. A record carries `seq`, `session_id`, `label`,
  `kind`, `source`, `text`, the agent's own notification (`agent`) and the question with its
  `request_id` (`question`), so `AnswerSessionPermission` can act on it, and `at_unix_ms`.
- **The cursor** is the decimal `seq` of the last record returned. Passing `next_cursor` back returns
  only what came after it, and it survives a daemon restart. The `next_cursor` of an empty page is
  the log's newest `seq` (`"0"` for a log that never held one), so polling from it neither repeats
  nor skips a record. A cursor that is not a decimal `seq` is refused naming it.
- **The window** is the newest 1000 records per project. A cursor older than it answers
  `lost: N`, the records evicted before they were read; it is never silently trimmed.
- **A wait** is capped at 20 s, the remote-call clamp, also on a call relayed to another host. A
  timeout answers `timed_out: true` and the cursor to wait from again. A home agent waiting in the
  tool is inside a tool call, so it reports `Running` and [auto-stop](project-home-auto-stop.md)
  does not stop it mid-wait.
- **Scope.** Records are those of the caller's operating-system user, never the home session's own.
  One host per call: `daemon_instance_id` forwards it to that host's log, and there is no merged view
  across hosts.
- **Tool guidance.** The descriptions tell the agent to wait with the last cursor, that a timeout means
  nothing happened, and never to wait in a loop with nothing to decide.

### Following a session

`ReadSessionActivity` and `ReadSessionTranscript` let an orchestrator follow a session of its project
the way a person follows it in the web UI, on any eligible host.

- **One paging contract, and no unpaged call.** Both take `cursor` (empty for the start) and `limit`
  (default 50, at most 200; a larger value is clamped, and `clamped_to` says so) and answer `items`,
  `next_cursor` and `has_more`. A cursor no page returned is refused as an `is_error` result naming
  `invalid cursor`.
- **Activity pages forward** by `activity_seq`, so polling with the last `next_cursor` returns
  exactly what happened since; on an empty page `next_cursor` is the cursor passed. It reads every
  call of the session, not the 500-call tail the session view shows. `statuses` filters by call state
  and still pages by sequence: `agent-activity.jsonl` records only tool calls, so state is the kind
  there is.
- **The transcript pages newest first** by default: an empty cursor is the newest page, each page is
  oldest first, and `next_cursor` is the page's first `seq` (pass it back for the page before).
  `from_cursor` reads forward instead, and its `next_cursor` is the position after the page. An
  item's `seq` is its position in the transcript `GetAcpReplayPage` pages, and its `kind` is
  `user_message`, `agent_message`, `agent_thought`, `tool_call` (its text is the call's title),
  `plan` or `other`. A claude-cli session's transcript holds tool calls only: its user and
  assistant text is not kept on the server
  ([TODO](../../dev/todo/2026-10-25-claude-cli-transcript-has-no-conversation-text.md)).
- **The pending question.** Every activity page carries `pending_question`, `{kind, header, question,
  options, allow_other}` or `null`. It is kept in the session directory as `pending-question.json`,
  beside the `pending_elicitation` flag, by two writers:
  - a **tool session's** own process, from its presenter's mode: a select (`kind: select`, with the
    clarification's header, options and `allow_other`), a multi-select (`multi_select`) or a
    free-text prompt (`text`). Any other mode clears it.
  - the **daemon**, for a **claude-cli session**: the session hook sends the agent's own `message`
    with the `WaitingForInput` status (`ReportSessionStatusRequest.message`), recorded as
    `kind: permission`. The next status — or a wait reported with no message — clears it.

### Terminating a session

`TerminateSession` stops a session of the caller's project, on any eligible host, exactly as the web
UI's Terminate does: the target's host sends **SIGTERM** to the process its `.session.yaml` records,
through the same code `SessionService.SignalSession` runs (`signal_session_dir` in
`tddy-agent-launch`), on the sessions directory the host resolved the target from.

- **Nothing else changes.** The session's record, its worktree and its branch stay; a terminated
  session can be resumed like any stopped one.
- **Refusals.** A session whose process is not alive, or that records no pid, is an `is_error` result
  (`process is not alive`, `session has no PID`); another project's session is refused like every
  target tool's and its process is left running.
- **Not offered:** `DeleteSession` (which removes the worktree), SIGINT and SIGKILL.

### Git tools

A project-home session can run git on **another session's worktree** in its project. Targets are
resolved as for the project-home tools: same project, never the home worktree (refused
naming `project-home`), and only a project-home caller may run them. They are in the project-home
session's offered set, so they are advertised to it and pre-approved in its `--allowedTools`.

| Tool | Arguments | Runs |
|---|---|---|
| `CommitSessionWorktree` | `session_id`, `message`, `paths?` (every change when empty) | always |
| `PushSessionBranch` | `session_id`, `force_with_lease?` | plain push always; force-with-lease only on an inactive target |
| `RebaseSessionBranch` | `session_id`, `onto` | inactive target |
| `StashSessionWorktree` | `session_id` (untracked files included) | inactive target |
| `ResetSessionWorktree` | `session_id`, `mode` (`soft`/`mixed`/`hard`), `to` | inactive target |
| `CheckoutSessionWorktree` | `session_id`, `to` | inactive target |
| `CleanSessionWorktree` | `session_id` | inactive target |

- **Activity gate.** Every operation that rewrites the target's files or history — and a
  force-with-lease push, which rewrites the remote branch its agent builds on — is refused while the
  target session is active, as an `is_error` result naming the session ("… is active; stop it
  before rewriting its worktree"). Commit runs beside an active agent; its next edit lands in a
  later commit.
- **Rebase conflicts** are a result, not an error: the rebase is aborted (`git rebase --abort`),
  the worktree is left as it was, and `result_json` carries `{"conflict_paths": [...], "aborted": true}`.
- **Identity.** Every git command runs with the `GIT_*` environment the target session itself
  starts with (its project's account, or the host's `gh`). When that does not resolve, the
  checkout's own git identity applies. A ref argument that starts with `-` is refused.
- **Push** goes to `origin` under the worktree's branch name, with the checkout's own credentials for
  an HTTPS remote ([TODO](../../dev/todo/2026-10-23-project-home-git-push-remote-and-token.md)).
- No merge, PR or branch-creation tools here — the `pr_*` tools cover those.

## Related

- [Cloning over SSH](clone-ssh-passphrase.md) — the passphrase question a clone asks.
- [Per-project GitHub identity](github-identity.md) — what a session does with the account its project assigns, or with the host's `gh` login for a Local gh project.
- [Git integration base ref (worktrees)](../coder/git-integration-base-ref.md) — validation, default ref, project registry fields.
- [gRPC remote control](../coder/grpc-remote-control.md) — daemon and transport roles.
- [Web terminal](../web/web-terminal.md) — Connection screen UI.
- [LiveKit peer discovery and host selection](livekit-peer-discovery.md) — **`ListEligibleDaemons`**, **`StartSession`** routing across daemons sharing **`livekit.common_room`**.
