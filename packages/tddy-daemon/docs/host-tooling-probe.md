# Host tooling probe (tddy-daemon)

What a host has installed and configured, as opposed to how busy it is.

`host_stats.rs` reports load. `host_tooling.rs` reports the two facts that decide whether work on a
host will actually succeed: the git identity its commits would carry, and whether the GitHub CLI
there is authenticated. It backs the web's Hosts rows
([docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)) through the
`GetHostTooling` RPC ([connection-service.md](./connection-service.md)).

This is the first *capability probe* in the daemon — the first place tddy asks a machine what is on
it rather than reporting something the daemon already holds.

## The seam

```rust
pub trait HostToolingProbe: Send + Sync {
    fn probe(&self, os_user: &str) -> HostTooling;
}
```

One method, taking the OS user to answer for. `SubprocessHostToolingProbe` is the live
implementation; `ConnectionServiceImpl::with_host_tooling` substitutes a deterministic double, the
same shape as `with_host_stats` and for the same reason: **a test must never depend on what happens
to be installed on the machine running it**, and asserting against the developer's own git identity
is the anti-pattern [the testing guide](../../../docs/dev/guides/testing.md) names. A test-only
branch inside the probe is not an alternative — CLAUDE.md forbids code paths that only work under
test, so the substitution point is a trait and nothing else.

`HostTooling` is `{ git: GitIdentity, github_cli: GithubCliStatus }`, and each half carries a
`ProbeOutcome` **before** its findings:

| `ProbeOutcome` | Meaning |
|---|---|
| `Ok` | the probe ran; the fields below it are a real answer |
| `Failed(reason)` | it could not run, or produced output this code does not understand |
| `Unsupported` | the platform cannot run it at all — the spawn-as-user path is Unix-only |

`GitIdentity.name_and_email` is `Option<(String, String)>`: `None` **with `Ok`** is the answer "this
host has no identity configured", which is not the same fact as "we could not ask". An identity is
the pair — git refuses to commit without both, so a host with one half set has no identity its
commits would carry, and rendering the half beside a blank would state something untrue.

## Classification is a pure function

`classify_gh_auth_status(exit_code: Option<i32>, output: &str) -> GithubCliStatus` is split out from
the subprocess call. The classification is the part that is easy to get wrong and expensive to get
wrong, and as a pure function of `(exit_code, output)` it is provable without spawning anything.

`gh auth status` writes to **stderr**, has moved between the two streams across releases, and its
wording is not a stable API — so the caller concatenates both streams and hands the lot to the
classifier, whose parse is deliberately narrow:

1. **A login is looked for first.** `gh auth status` reports every host it knows, so one output can
   carry both a login and a logged-out phrase; the honest reading of "logged in to one, out of
   another" is authenticated. `parse_gh_login` requires both anchors on one line — `Logged in to`
   and ` account ` — because the first alone also appears in `gh`'s own suggestion text, where
   matching it would invent a login out of the next word.
2. **Then a logged-out phrase.** `not logged in` covers both wordings `gh` uses.
3. **Anything else is `Failed`**, carrying a one-line excerpt of what was not understood.

Step 3 is the rule the module exists for: **unrecognised output is a probe failure and never a
negative finding.** Reporting an authenticated host as logged out sends an operator to re-authorise
something that is not broken.

`exit_code: None` means one thing only — there was no `gh` to run, because the caller looked for it
and found nothing. That is the sole evidence this code accepts for "not installed" (see below).

`parse_git_identity(name_output, email_output)` is the equivalent for git: both values trimmed, an
identity only when both are non-empty.

## Resolving `git` and `gh`

Both programs are resolved to absolute paths **before** anything is spawned, via
`spawner::find_program_on_spawn_child_path`.

This is not incidental. `spawner`'s `resolve_tool_path` anchors a *relative* program name to the
**daemon's own process cwd** and never consults `PATH` — deliberately, so an operator's configured
tool path cannot be shadowed by whatever is on a target user's `PATH`, and pinned by
`run_capture_as_user_locates_a_relative_program_path_against_the_daemons_own_cwd_not_the_target_users_home`.
A bare `"git"` therefore execs `<daemon-cwd>/git`, and the `--systemd` unit sets no
`WorkingDirectory=`, which makes that `/git`.

`find_program_on_spawn_child_path` searches the same `PATH` the child will actually be given
(`merge_spawn_child_path`) for an executable file. `None` from it is a *finding*, and what the
finding means is the caller's to decide — which is the whole reason the lookup is a separate
function from the spawn:

- **`gh` absent from that `PATH` is "not installed"**, and it is the only evidence accepted for it.
- **`git` absent is a probe failure**, never "no identity configured". The host's `~/.gitconfig` may
  well hold one; nothing read it.

Everything that goes wrong *after* a successful lookup is `Failed`, including `ErrorKind::NotFound`
from the spawn — by then the binary has been found, and that error also covers a working directory
that does not exist (the probe chdirs into the target user's home) and an exec that could not be set
up. None of those is evidence of absence.

## Running as the host's OS user

Both facts are per-user: `git config --global` reads `$HOME/.gitconfig` and `gh auth status` reads
`$HOME/.config/gh/hosts.yml`. A probe run as the daemon's own user answers for a different account,
so both go through `spawner::start_output_as_user`, which resolves the user with `getpwnam_r`, sets
`HOME` and `PATH`, chdirs into that home, and drops privilege in `pre_exec`
(`setgid` → `initgroups` → `setuid`). `as_user_command` holds that setup once, shared with
`run_output_as_user`, because a second copy that drifted would produce a child running as the wrong
user or reading the wrong `$HOME` — precisely the answer these callers exist to get right.

`--global` is load-bearing on the git reads. Without it, git also reads the **repo-local** config
for the process's cwd — and the probe's cwd is the target user's home, which is itself a work tree
whenever that user keeps their dotfiles in git. A repo-local `user.name` there would shadow the
global one, and the row would report an identity that commits made anywhere else on the host would
not carry.

Exit codes are classified rather than collapsed: `git config --get` exits **1** for an unset key,
which is an empty value and not an error — treating it as a failure would hide the very state the
probe exists to report. Any other code is a failure, and so is a signal.

## One deadline, and a child that is actually ended

`PROBE_TIMEOUT` is 5 s for the whole probe, not per command. All three commands (`user.name`,
`user.email`, `gh auth status`) are **started** before any is collected, so the Hosts screen waits
for the slowest rather than the sum, and a `gh` that reaches the network cannot decide how long the
git answer takes.

The shape that makes the deadline enforceable:

- `start_output_as_user` returns the **live `Child`** with its two pipes already detached. The
  parent owns the deadline, so the parent has to be the side that can end the child — and it ends it
  through the `Child` itself, never through a saved pid, because a pid outlives the process it named
  and the next signal lands on whatever the kernel has since reused it for.
- A reader thread gets **only the pipes**, and reads both concurrently for the reason
  `Command::output` does: a child that fills one pipe's buffer while the reader blocks on the other
  never drains, and would hang until the deadline killed it — turning a ready answer into a timeout.
- On deadline the parent **kills and reaps** — `kill()` then `wait()`. Kill alone leaves a zombie.
  Killing the child closes both pipes, both reads hit EOF, and the reader thread ends on its own.

`run_output_as_user` cannot serve this: it waits unconditionally and hands back no handle, so a
caller that gave up left the process, the waiting thread and two descriptors alive past its own
deadline. For a daemon that probes every host it lists, on every poll, that is a descriptor leak
rather than a slow answer.

A timed-out command is `Failed`, like every other `Err` on this path.

## Failures are logged

`Failed` is the one outcome whose cause lives on the daemon's side of the wire — a missing tool, a
refused spawn, a timeout. `warn_if_failed` writes one `log::warn!` per failed half, naming the OS
user and the reason, because without it the only trace of a broken probe is a cell on a screen
nobody may be looking at.

## Non-Unix

`start_output_as_user` is Unix-only, so `probe` on every other target returns
`ProbeOutcome::Unsupported` for both halves. That is a property of the platform, not of the host's
tooling, and it says so rather than surfacing an internal error an operator would try to fix.

## Testing

| Level | Where | What only exists there |
|---|---|---|
| Unit | `host_tooling.rs` `#[cfg(test)]` | classification per outcome, and the timeout's kill-and-reap |
| Integration | `connection_service.rs` `#[cfg(test)]` | auth rejection, the OS user the probe is handed, peer routing |
| Component | `packages/tddy-web/cypress/component/HostsScreenToolingAcceptance.cy.tsx` | the states rendering distinguishably |

Shelling out to the real `git` / `gh` was rejected: CI has an arbitrary `gh` state, and a suite that
asserts against it is environment-dependent by construction.

`ProbeOutcome::Failed` carries its reason as a `String` for an operator to read, not for a caller to
parse. Nothing branches on the text.

## See also

- [connection-service.md](./connection-service.md) — the `GetHostTooling` RPC and its routing
- `host_stats.rs` — the other per-host reader, documented in
  [connection-service.md § Host stats](./connection-service.md#host-stats); this module copies its
  injectable-trait shape
- Feature: [docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)
- Web: [packages/tddy-web/docs/hosts-screen.md](../../tddy-web/docs/hosts-screen.md)
