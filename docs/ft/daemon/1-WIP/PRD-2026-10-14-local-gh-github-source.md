# Local gh as a project's GitHub source - PRD

**Date**: 2026-10-14
**PRD Type**: Enhancement
**Status**: 🔴 Specified by failing tests; not implemented. Every acceptance criterion below is open.

## Affected Features

- **Primary Feature**: [Per-project GitHub identity](../github-identity.md) — a second source for
  what a session acts as: the host's own GitHub CLI login, instead of an account in the vault.
- **Related Feature**: [Project concept](../project-concept.md#account-assignment) — a project gains a
  **Local gh** setting beside its account assignments.
- **Related Feature**: [Projects screen (multi-host)](../../web/projects-screen-multi-host.md#accounts)
  — the toggle, and each host's `gh` login state on the project card.
- **Related Feature**: [Telegram session control](../telegram-session-control.md) — a Telegram-started
  session of a Local gh project acts as the host's `gh` login (today it acts as nobody).
- **Related Feature**: [Hosts screen tooling](../../web/hosts-screen-tooling.md) — the `gh` probe is
  reused per project hostname; no change to the Hosts screen itself.
- **Related Feature**: [LiveKit peer discovery](../livekit-peer-discovery.md) — a forwarded session
  start carries the project's GitHub source.
- **Unchanged**: [Account linking](../account-linking.md), [Accounts screen](../../web/accounts-screen.md),
  [Credential sync](../credential-sync.md), dashboard sign-in ([session auth](../session-auth.md)).

## Summary

A project can be set to **Local gh**. Its sessions then do not use a GitHub account from the
signed-in person's vault. Each host instead uses its own GitHub CLI, which the person has logged in
on that host with `gh auth login`, as the OS user the session runs as. The agent runs `gh` itself.
tddy's own GitHub steps also read the host's `gh` where they run: the PR tools, merging and
re-pointing a PR stack, and re-pointing a planned PR. The session's commits carry the `gh` login's
identity, and no token is handed out through tddy.

## Background

Today the only way a session acts as a GitHub user is a vault account: link it through the device
flow, assign it to the project, and keep the vault unlocked. That is the right model for a shared
multi-user daemon. It is heavy for the common single-person case, where every host the person uses
already has a working `gh auth login` and they want tddy to simply use it. It also leaves two
situations with no identity at all:
- a session started from **Telegram**, which has no vault to open;
- a **desktop** install whose vault is locked or does not exist yet.

## Proposed Changes

### What's Changing

**A project setting, not an account.** A project has a **Local gh** toggle on its card.
- Turning it on removes the project's GitHub account assignment in the same change. Other providers'
  assignments stay.
- While the toggle is on, the GitHub account picker is not offered.
- Turning it off leaves the project with no GitHub account assigned.
- A Local gh project with a GitHub assignment is refused, naming the conflict, so the two sources
  can never both be in effect.

**What a Local gh session acts as**

| What | Source |
|---|---|
| Commit author and committer name | the `gh` login on the session's host, for the project's GitHub host |
| Commit author and committer email | `<GitHub id>+<login>@users.noreply.github.com`, from that same login |
| Token for GitHub API calls | read from `gh` by whatever needs it, at the moment it needs it, as the session's OS user |

**The project's GitHub host comes from its git URL.** For example, `git@github.example.com:org/repo.git`
is asked of `gh` as `github.example.com`, so GitHub Enterprise works. A git URL whose host cannot be
read is refused with that reason; it is never assumed to be `github.com`.

**The identity is read once per start or resume**, on the host the session runs on, by asking `gh`
which user it is logged in as there. Like a vault account, it is kept for the session's lifetime. A
`gh auth login` as someone else takes effect at the next start or resume.

**The token is never delivered.**
- The agent runs `gh` itself.
- tddy's PR tools and the workflow's GitHub steps read the token from `gh` in their own process, as
  the session's OS user, each time they need it.
- The daemon-side re-point of a planned PR asks `gh` as the session's OS user.
- No token crosses tddy's session socket, and none is written to an environment variable or a file.
  The session is told only *that* it uses Local gh, and for which host.

**Every host answers for itself, and a forwarded start says which source to use.**
- When a session start is forwarded to another host, it carries the project's GitHub source as the
  forwarding host knows it. The target host uses `gh` even if its own copy of the project was never
  set.
- A session started with an explicit source remembers it, so a resume on that host acts the same way.
  A session started without one re-reads the host's project setting on resume, exactly as an account
  assignment is re-read today.
- A start made directly on a host uses that host's own setting.

**When `gh` cannot answer, the session still starts.** This is the same consented exception as an
unassigned project. The cases are: `gh` missing, logged out for the project's host, or failing.
- The session starts under the checkout's own git identity.
- The reason is logged and shown, naming what to do, e.g. *"gh on workstation-2 is not logged in to
  github.com as ada; run `gh auth login` there"*.
- Each GitHub step that needs a token then fails with `gh`'s own reason.

**The environment still rescues nothing.** `GITHUB_TOKEN` and `GH_TOKEN` are removed from every
process a session runs: the agent, its shell commands and its tools. `gh` prefers those variables
over its own login, so without this a token exported in the daemon's environment would silently stand
in for the host's login.

**Sandboxed sessions**

| Daemon runs as the session's OS user (desktop, `--user` installs) | Daemon serves several OS users |
|---|---|
| Fully supported: the agent's shell commands, which run on the host, use the session user's `gh` | The session starts with no identity and a reason. `gh` in its shell commands is deliberately logged out, so it can never act as the daemon user's login |

tddy's PR tools *inside* a jail cannot reach GitHub (the jail has no network), so they report that.

**Telegram.** A Telegram-started session of a Local gh project acts as the host's `gh` login, with
identity and token reading exactly as a dashboard start. The "start from the web dashboard" notice is
not sent for these projects.

**The project card shows each host's `gh` state** while the toggle is on, for the project's GitHub
host: *logged in as `<login>`*, *not logged in*, *gh not installed*, or *could not check (reason)*.
This is the same probe the Hosts screen uses, asked per host and per hostname.

### What's Staying the Same

- Vault accounts, account linking, the Accounts screen and credential sync are untouched. A project
  without the toggle behaves exactly as today.
- Signing in to the dashboard is still GitHub OAuth.
- Work snapshots (WIP commits) are still signed by `tddy-daemon`.
- Pushes still use the host's git credentials, as today. Setting up `gh auth setup-git` is the
  person's choice, not tddy's.
- A project's account assignment still reaches only the host it was written to. Only the Local gh
  fact gains a way to travel, via the forwarded start.

## Impact Analysis

### Technical Impact

- **Project row and wire.**
  - New `local_gh` on the stored project and on `ProjectEntry`.
  - `SetProjectAccounts` carries it, so the toggle and the assignment change in one write. No new RPC.
- **Session start.**
  - `StartSessionRequest` carries the GitHub source.
  - A session started with an explicit source records it next to its metadata, so its resume keeps it.
- **Identity resolution.**
  - New resolution: ask `gh` as the OS user for the project's hostname, then derive the git identity.
  - The four `GIT_*` pairs are built from a git identity, not only from a vault record.
- **Token readers.**
  - The session's process learns "Local gh for `<host>`" from one new environment entry. It is not a
    secret.
  - The shared token client reads `gh` when that entry is present, and never also asks the socket.
  - The spawn wire and the supervisor policy admit that one key.
- **Environment hygiene.** `GH_TOKEN` and `GITHUB_TOKEN` are removed from:
  - PTY agent spawns;
  - the host-side shell relay;
  - `spawn_as_user`.
- **Host probe.** `gh auth status`/`gh api user` asked per hostname; `GetHostTooling` takes an
  optional hostname.
- **Web.** The toggle, the hidden picker, and per-host `gh` state on the project card.

### User Impact

- Single-person and desktop setups get a working GitHub identity with no vault and no device-flow
  link, as long as each host has `gh auth login` done.
- Telegram-started sessions of Local gh projects can push PRs and commit under a real identity.
- On a multi-user daemon, sandboxed Local gh sessions run with no identity, by design.

## Implementation Plan

See the changeset [2026-10-14-local-gh-github-source.md](../../../dev/1-WIP/2026-10-14-local-gh-github-source.md).

## Acceptance Criteria

- [ ] A project can be switched to Local gh from its card; doing so removes its GitHub account
      assignment and keeps other providers' assignments.
- [ ] A Local gh project with a GitHub account assignment is refused with a reason naming both.
- [ ] A Local gh session on a host where `gh` is logged in commits as that login's noreply identity,
      for the project's git host (github.com and an Enterprise host).
- [ ] A Local gh session's tools and workflow steps obtain the token from `gh` in their own process;
      no token is in any session environment, and the session socket never answers with one.
- [ ] Re-pointing a planned PR of a Local gh session asks `gh` as the session's OS user.
- [ ] Where `gh` is missing, logged out or failing, the session starts under the checkout's own
      identity and the reason names the host and what to run.
- [ ] A `GH_TOKEN` / `GITHUB_TOKEN` exported in the daemon's environment reaches no session process,
      agent shell command or `gh` probe.
- [ ] A session start forwarded to another host acts as Local gh even when that host's own project
      row was never set, and its resume does too.
- [ ] A sandboxed Local gh session works when the daemon runs as the session's OS user; otherwise it
      starts with no identity and `gh` in its shell commands is logged out.
- [ ] A Telegram-started Local gh session acts as the host's `gh` login and is not sent the "start from
      the dashboard" notice.
- [ ] The project card shows, per host, whether `gh` is logged in for the project's GitHub host, and as
      whom.

## References

- [Per-project GitHub identity](../github-identity.md)
- [Project concept](../project-concept.md)
- [Projects screen (multi-host)](../../web/projects-screen-multi-host.md)
- [Hosts screen tooling](../../web/hosts-screen-tooling.md)
- [Telegram session control](../telegram-session-control.md)
- [Credential sync](../credential-sync.md) — what does propagate between daemons (vault records)
