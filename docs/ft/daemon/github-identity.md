# Per-project GitHub identity

**Status:** Current

## Summary

Every git and GitHub operation a daemon performs for a session acts as **the account the session's
project is assigned** — the token and the commit identity, from one resolution. Two projects on one
daemon, assigned different accounts, act as different GitHub users.

A project can instead be set to **[Local gh](#local-gh)**: its sessions then act as the GitHub login
of the host's own GitHub CLI, with no vault account involved.

The process environment is not a credential. `GITHUB_TOKEN` and `GH_TOKEN` are never read, in the
daemon or in a session, and exporting them rescues nothing.

## What a session acts as

| What | Source |
|---|---|
| Commit author and committer name | the account's GitHub login |
| Commit author and committer email | `<GitHub id>+<login>@users.noreply.github.com` |
| Token for GitHub API calls made by tddy | the account's record in the signed-in person's vault |

The account is the one the project assigns ([project concept](project-concept.md#account-assignment)),
looked up among the accounts the person's own vault holds. It is chosen **once**, when the session
starts or resumes, and the session keeps it: reassigning the project, or linking a second account,
takes effect the next time the session is started or resumed. Renaming an account does not change who
its commits are authored by.

Work snapshots the daemon takes of a session (WIP commits) are still signed by `tddy-daemon`: they are
machine-made snapshots of the agent's work, not the agent's work.

## How the agent gets the token

The token is **asked for, never delivered**. The agent's GitHub tools ask the session's host for the
token at the moment they run; the host resolves the session's account and answers that one call. The
token is in no environment variable and no file, and is kept out of logs. The same applies to the
workflow's own GitHub steps (merging and re-targeting a PR stack): they ask only when a step that
reaches GitHub runs, so a plan-only run never asks and a refusal surfaces at the step that needed the
token.

A tool session started by the daemon reaches its host over an owner-only socket per OS user. A session
that stopped is no longer answered; a session that was running when the daemon restarted is told to
resume it.

## When there is no account to act as

Each outcome is refused with its own reason, naming what the person would do about it:

| Situation | Reason given |
|---|---|
| the project assigns no GitHub account | not assigned |
| assigned, but this host's vault does not hold it | unknown on this host |
| two accounts assigned at one provider | ambiguous — one is never picked |
| the account's record carries no GitHub identifiers | unusable, naming what is missing |
| the vault is locked, uninitialized, or the person's session ended | the vault's own reason |

GitHub tools and daemon-side GitHub operations (such as re-pointing a planned PR) are refused with
that reason. **The session still starts**, without account commit identity, and the refusal is
recorded in the daemon log; commits then carry the checkout's own `user.name` and `user.email`. This is
the one place the identity is not strictly enforced; it keeps unassigned projects working. No token is
delivered on that path, so a push cannot succeed as the wrong account. A session started refused stays
refused until it is resumed, even if the person then unlocks the vault or assigns an account.

## Local gh

A project has a **Local gh** setting, beside its account assignments
([project concept](project-concept.md#local-gh)). Its sessions do not use a vault account. Each host
uses its own GitHub CLI, which the person has logged in on that host with `gh auth login`, as the OS
user the session runs as. It suits a single person whose hosts already have a working `gh`, a
desktop whose vault is locked or does not exist yet, and a session started from Telegram.

| What | Source |
|---|---|
| Commit author and committer name | the `gh` login on the session's host, for the project's GitHub host |
| Commit author and committer email | `<GitHub id>+<login>@users.noreply.github.com`, from that same login |
| Token for GitHub API calls | read from `gh` by whatever needs it, at the moment it needs it, as the session's OS user |

**The project's GitHub host comes from its git URL.** `git@github.example.com:org/repo.git` is asked
of `gh` as `github.example.com`, so GitHub Enterprise works. A git URL whose host cannot be read is
refused with that reason; it is never assumed to be `github.com`.

**The identity is read once per start or resume**, on the host the session runs on, by asking `gh`
which user it is logged in as there, and is kept for the session's lifetime. A `gh auth login` as
someone else takes effect at the next start or resume.

**tddy's token and the agent's own `gh`.** The agent runs `gh` itself, with its own login. tddy's own
GitHub steps, the PR tools, the workflow's merging and re-pointing of a PR stack, and re-pointing a
planned PR, read the token from `gh` where they run, as the session's OS user. No token crosses
tddy's session socket, and none is written to an environment variable or a file; the session is told
only *that* it uses Local gh, and for which host.

**Which source a session uses.** A session start forwarded to another host carries the project's
source as the forwarding host knows it, so the target host uses `gh` even if its own copy of the
project was never set. A session started with an explicit source remembers it, and a resume on that
host acts the same way. A session started without one re-reads the host's project setting on resume,
as an account assignment is. A start made directly on a host uses that host's own setting.

**When `gh` cannot answer** (missing, logged out for the project's host, or failing) the session still
starts, under the checkout's own git identity: the same consented exception as an unassigned project.
The reason is logged and names what to do, for example *"gh is not logged in to github.com as OS user
ada; run `gh auth login --hostname github.com` as ada"*. Each GitHub step that needs a token then
fails with `gh`'s own reason.

**The environment still rescues nothing.** `GITHUB_TOKEN`, `GH_TOKEN` and their Enterprise
counterparts are removed from every process a session runs, the agent, its shell commands and its
tools, and from every `gh` tddy runs. `gh` prefers those variables over its own login, so without this
a token exported in the daemon's environment would silently stand in for the host's login.

**Sandboxed sessions**

| Daemon runs as the session's OS user (desktop, `--user` installs) | Daemon serves several OS users |
|---|---|
| Fully supported: the agent's shell commands, which run on the host, use the session user's `gh` | The session starts with no identity and a reason. `gh` in its shell commands is deliberately logged out, so it can never act as the daemon user's login |

tddy's PR tools *inside* a jail cannot reach GitHub, because the jail has no network, and report that.

**Limits.** Child sessions of a session (stack children, conversations) read the host's own project
setting rather than the parent's forwarded source
([TODO](../../dev/todo/2026-10-15-local-gh-child-sessions-and-telegram-gh.md)). Pushes still use the
host's git credentials; setting up `gh auth setup-git` is the person's choice, not tddy's.

## Where this does not apply

- A session started from **Telegram** has no signed-in person to open a vault for. It starts under the
  checkout's identity with no GitHub token, and when the project assigns an account the chat is told
  so and told that starting from the web dashboard acts as the account. A **Local gh** project needs
  no vault, so its Telegram start acts as the host's `gh` login and is not told this.
- A plain Cursor CLI session has no tool channel, so it carries the commit identity but no PR-tool
  token.
- A split agent makes no commits itself.

## Supervised hosts

Where the daemon runs as an unprivileged child of `tddy-supervisor`, each session user's socket is
declared in `supervisor.yaml` (`host_sockets:`) and the supervisor must allow the four git identity
environment keys for session spawns, plus `TDDY_GITHUB_SOURCE` for projects that use Local gh. A user who is not declared gets no GitHub token in their tool
sessions, and the refusal names the setting. That path is built and unit-tested; it has not yet run
under a real root supervisor.

## Related

- [Project concept](project-concept.md) — account assignment
- [Account linking](account-linking.md) — how a second account gets into the vault
- [GitHub pull request tools](../coder/github-pr-tools-mcp.md)
- [Telegram session control](telegram-session-control.md)
- Technical: [identity resolution](../../../packages/tddy-accounts/docs/github-identity-resolution.md),
  [session identity](../../../packages/tddy-session-lifecycle/docs/session-identity.md),
  [the `gh` questions](../../../packages/tddy-host-service/docs/host-tooling-probe.md#the-github-cli)
