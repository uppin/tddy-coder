# Per-project GitHub identity

**Status:** Current

## Summary

Every git and GitHub operation a daemon performs for a session acts as **the account the session's
project is assigned** — the token and the commit identity, from one resolution. Two projects on one
daemon, assigned different accounts, act as different GitHub users.

The process environment is not a credential. `GITHUB_TOKEN` and `GH_TOKEN` are never read, in the
daemon or in a session, and exporting them rescues nothing.

## What a session acts as

| What | Source |
|---|---|
| Commit author and committer name | the account's GitHub login |
| Commit author and committer email | `<GitHub id>+<login>@users.noreply.github.com` |
| Token for GitHub API calls and pushes | the account's record in the signed-in person's vault |

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

## Where this does not apply

- A session started from **Telegram** has no signed-in person to open a vault for. It starts under the
  checkout's identity with no GitHub token, and when the project assigns an account the chat is told
  so and told that starting from the web dashboard acts as the account.
- A plain Cursor CLI session has no tool channel, so it carries the commit identity but no PR-tool
  token.
- A split agent makes no commits itself.

## Supervised hosts

Where the daemon runs as an unprivileged child of `tddy-supervisor`, each session user's socket is
declared in `supervisor.yaml` (`host_sockets:`) and the supervisor must allow the four git identity
environment keys for session spawns. A user who is not declared gets no GitHub token in their tool
sessions, and the refusal names the setting. That path is built and unit-tested; it has not yet run
under a real root supervisor.

## Related

- [Project concept](project-concept.md) — account assignment
- [Account linking](account-linking.md) — how a second account gets into the vault
- [GitHub pull request tools](../coder/github-pr-tools-mcp.md)
- [Telegram session control](telegram-session-control.md)
- Technical: [identity resolution](../../../packages/tddy-accounts/docs/github-identity-resolution.md),
  [session identity](../../../packages/tddy-session-lifecycle/docs/session-identity.md)
