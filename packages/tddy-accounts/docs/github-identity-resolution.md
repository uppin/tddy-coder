# GitHub identity resolution

## What it answers

Which GitHub account a project acts as, for the token **and** the commit identity, from one
`acting_identity(assignments, provider, held)` call (`src/identity.rs`). `assignments` are the project
row's `accounts`; `held` are the GitHub records the signed-in person's vault holds right now.

The crate publishes no way to obtain the token without the identity, or the identity without the
token: both halves are fields of one `ActingIdentity { account, token, git }`. There is nothing to
police, because two independent resolutions cannot be written.

## Outcomes

| Resolution | Result |
|---|---|
| `Assigned` | `ActingIdentity { account, token, git }` |
| `NotAssigned` | `IdentityError::NotAssigned` — the project assigns no GitHub account |
| `UnknownOnThisHost` | `IdentityError::UnknownOnThisHost` — assigned, but this host holds no such account |
| `Ambiguous` | `IdentityError::Ambiguous` — two accounts at one provider; never picks |
| `Assigned`, record carries no provider identifiers | `IdentityError::Unusable` — names the absent metadata key |

Each refusal is a distinct message (`IdentityError: Display`) that names what the person would do
about it. `Unusable` is the one outcome beyond the resolver's four: a record can resolve and still
carry no provider identifiers, and refusing it is the alternative to inventing an address — a commit
authored under an invented address is attributed to nobody.

## Identity derivation

The name is the login (`META_SUBJECT`); the email is
`{META_SUBJECT_ID}+{login}@users.noreply.github.com`. The record's label is mutable and is **never**
used, so renaming an account does not change who its commits are authored by.
`tddy_daemon_livekit::session_git_environment` renders the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*`
pairs from one `ActingIdentity`; the token is never among them. It returns pairs rather than applying
them to a `Command`, because the caller that spawns the process is the only one that knows which
process they belong on, and a list of pairs is readable without running `git`.

## Where the resolution is taken

At the session's edge, from `tddy-session-lifecycle`: `SessionAccountAccess::session_identity` returns
the commit pairs and the handler that answers the agent's tools' `github-token` request
([session identity](../../tddy-session-lifecycle/docs/session-identity.md)). The pairs are applied to
the agent's process environment (co-located), to the host-side relay environment (jailed), or to the
child's environment over the spawn wire (tool sessions). The token is **asked for, per call**, over a
socket the session's host serves — never an environment variable, never a file. Daemon-side
operations (`RepointPlannedPr`) call `project_github_token` directly.

`tddy-accounts` names `tddy-credentials`, `tddy-rpc` and `tddy-service` and nothing about sessions or
LiveKit: the crate deciding which account a project acts as does not know who asks.

## Local gh

A project set to Local gh does not go through `acting_identity`. The `local_gh` module
(`src/local_gh.rs`) holds the two pure steps that need no vault:

- `github_hostname_of(git_url)` gives the GitHub host the project's git URL names, for the https,
  scp-style ssh (`git@host:org/repo.git`) and `ssh://` forms, without user, port or path. The host
  must be a DNS name or a bracketed IPv6 literal; anything else, including an empty host or one
  starting with `-`, is `LocalGhError::UnreadableGitUrl`, whose message tells the person to set a
  git URL such as `https://github.com/<owner>/<repo>.git`. The host is never assumed to be
  `github.com`, because the value reaches `gh --hostname` and the `TDDY_GITHUB_SOURCE` entry.
- `local_gh_git_identity(login, id)` gives the same noreply identity shape as a vault account:
  name `login`, email `<id>+<login>@users.noreply.github.com`.

Asking `gh` who it is logged in as, and for its token, is
[`GithubCli`](../../tddy-host-service/docs/host-tooling-probe.md#the-github-cli) in
`tddy-host-service`; this crate stays free of subprocesses.

## No environment

Nothing in `tddy-github` or `tddy-workflow-recipes` reads `GITHUB_TOKEN` or `GH_TOKEN`; a token
reaches a REST call only as a parameter. A project that assigns no account is refused although the
daemon's environment holds a token. Pinned structurally by
`no_crate_still_resolves_a_github_token_from_the_process_environment` (`tddy-daemon-auth`) and
behaviourally by the `GITHUB_TOKEN`-is-set tests at every seam.

## One consented exception

A project that resolves no account does not stop a session starting: it starts with no `GIT_*`
variables, under the checkout's own identity, and the reason is logged at `warn`. It is not a token
fallback — no credential is delivered on any path. The same holds for a Local gh project whose `gh`
cannot answer.

## Not identity-bearing

WIP snapshots are signed by `tddy-daemon` under a fixed machine identity, because the object is a
machine-made snapshot of the agent's work, not the agent's work. A project having an account does not
change that.

## Tests

`tests/acting_identity_unit.rs` (each outcome, each message, the derivation, the label never used),
`tests/project_resolved_identity_acceptance.rs` (two projects, two accounts, one daemon; token and
identity name the same account; `GITHUB_TOKEN` rescues nothing).
