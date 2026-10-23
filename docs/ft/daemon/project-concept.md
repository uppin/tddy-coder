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

## Related

- [Cloning over SSH](clone-ssh-passphrase.md) — the passphrase question a clone asks.
- [Per-project GitHub identity](github-identity.md) — what a session does with the account its project assigns, or with the host's `gh` login for a Local gh project.
- [Git integration base ref (worktrees)](../coder/git-integration-base-ref.md) — validation, default ref, project registry fields.
- [gRPC remote control](../coder/grpc-remote-control.md) — daemon and transport roles.
- [Web terminal](../web/web-terminal.md) — Connection screen UI.
- [LiveKit peer discovery and host selection](livekit-peer-discovery.md) — **`ListEligibleDaemons`**, **`StartSession`** routing across daemons sharing **`livekit.common_room`**.
