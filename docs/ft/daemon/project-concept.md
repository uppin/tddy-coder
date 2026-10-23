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
| `home_agent` | Optional. The agent (a `CatalogService.ListAgents` id) the project's project-home session runs; see [Home agent](#home-agent). Absent means none is configured. |
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

## Home agent

A project can name the agent its **project-home session** runs, chosen from the Projects screen.

- **Set with `SetProjectHomeAgent`** (`project_id`, `home_agent`, `daemon_instance_id`), routed to one
  host exactly like `SetProjectDefaultBranch`. An empty `home_agent` clears the setting.
- **Only a listed agent is stored.** A non-empty id the writing daemon's `ListAgents` does not list is
  refused with `InvalidArgument` and nothing is written — a typo surfaces where it was entered, not
  when the home session later fails to start. A forwarded write is checked against the peer's own
  catalog.
- **Rows written by older daemons** have no `home_agent` key and read as unset; clearing writes no key.
- `ListProjects` reports it as `ProjectEntry.home_agent` (empty = unset).
- Like the account list, the write reaches one host (see
  [the open TODO](../../dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md)).

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

A project has at most one **project-home session** per host: a standing, **read-only**,
managed-codebase session running the project's **home agent**, pinned first in the project's group in
the session drawer ([Session drawer § Pinned main entry](../web/session-drawer.md#pinned-main-entry)).

`SessionService.EnsureProjectHomeSession(project_id, daemon_instance_id)` owns its lifecycle and
answers `{ session, outcome }`:

| What exists for the project on that host | Outcome |
|---|---|
| No home agent set | refused, `FAILED_PRECONDITION` ("set one on the Projects screen"); nothing starts |
| A home agent that is not a harness profile, or whose harness cannot enforce managed-codebase mode (anything but `claude-cli`; see [the cursor-cli TODO](../../dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md)) | refused, `FAILED_PRECONDITION` naming the harness; nothing starts |
| No project-home session | the **home worktree** is created and a managed session is started on it with the home agent → `started` |
| A stopped one | the home worktree is fast-forwarded to the fetched main branch ref, then the session is resumed — same conversation → `resumed` |
| A running one (its pid is alive, or the daemon still serves its terminal) | returned unchanged → `running` |

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
  not rest on the agent's allowlist.
- **What the agent is offered.** `ExecToolService.ListExecTools` takes a `session_id` and answers the
  set that session's agent is offered, read from the session's own record: the read-only tools plus
  the project-home tools for a project-home session, the catalog without the project-home tools for
  any other. `tddy-tools` asks it on every `tools/list` over every transport (the daemon's HTTP relay
  and unix socket, the sandbox socket — the runner rebinds the question to the jail's own session —
  and LiveKit) and advertises only what the daemon admits. A failed or timed-out ask (10 s) fails
  `tools/list` with an error naming the reason, rather than offering a catalog of its own. The
  co-located claude-cli home agent is also started (and resumed) with one `--allowedTools` per tool of
  that set, which pre-approves them; it does not restrict — enforcement stays the daemon's.
- **Hosts.** A request whose `daemon_instance_id` names another daemon is forwarded to it (as
  `GetWorktreeSnapshot` is); empty or the local instance is answered locally. Each host has its own
  home session for a project.
- Ensures are serialized on a daemon, so two clicks never start two home sessions. Deleting the
  home session is allowed; the next ensure starts a new one. Because it is managed, it opens with the
  activity sidebar.
- The "running" decision is a heuristic with one known false positive — a live pid with no served
  terminal ([TODO](../../dev/todo/2026-10-23-project-home-running-heuristic.md)).

### Project-home tools

Four exec tools give the read-only home agent its reach over **its own project's** sessions. Only a
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
