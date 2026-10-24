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
3. Otherwise: run `git clone` as the target OS user, then register the project.

## Cloning at session start

A session start on a host that does not hold the project's checkout clones it first, from the registered
`git_url` or from a peer's, into `{home}/{repos_base}/{name}/`. A clone over SSH asks the operator for a key
passphrase when the key is protected; see [cloning over SSH](clone-ssh-passphrase.md).

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
  `ListProjectSessions` / `StartProjectSession`, inspects them with `ReadSessionWorktree` /
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

Four exec tools give the read-only main its reach over **its own project's** sessions. Only a
session whose record carries the `project-home` role may run them; any other caller is refused with
`PERMISSION_DENIED` naming the role.

| Tool | Arguments | Answers |
|---|---|---|
| `ListProjectSessions` | — | `{"sessions": [{session_id, branch, worktree, is_active, agent_status, pending_elicitation}]}`: every other session of the project, with the status the web's session list shows |
| `StartProjectSession` | `session_type` (required), `prompt`, `agent`, `new_branch_name` | `{"session_id"}`: a session of any type `StartSession` accepts, started in the project as the caller's user; its changeset records the home session as `orchestrator_session_id`. A role cannot be passed |
| `ReadSessionWorktree` | `session_id`, `path` (relative; empty for the root) | a file's text (`content`, `truncated`, `byte_size`) or a directory's `entries`, through the same worktree reader as the Code pane |
| `DiffSessionWorktree` | `session_id` | `{session_id, base_ref, diff}`: the worktree's diff against its integration base (the session's recorded base, else the project's main branch ref), including uncommitted and untracked files. Taken over a temporary index; the worktree's own index is never written |

- **Targets.** A target is a session id on the same daemon and in the caller's project, with a
  worktree. Another project's session, the home session itself, an unknown id and a session without a
  worktree are refused as an `is_error` result naming why ("another project", …).
- **Paths.** An absolute path, one climbing above the worktree, or one resolving through a symlink to
  outside it is refused as an `is_error` result naming "outside the worktree". A base ref that git
  would read as an option is refused too.
- Every call is appended to the home session's tool-call log, as any exec tool is.

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
