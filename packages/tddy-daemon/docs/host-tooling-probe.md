# Host tooling probe (tddy-daemon)

What a host has installed and configured, as opposed to how busy it is.

`host_stats.rs` reports load. `host_tooling.rs` reports the three facts that decide whether work on
a host will actually succeed: the git identity its commits would carry, whether the GitHub CLI there
is authenticated, and whether an ssh-agent is holding a key its remotes would accept. It backs the
web's Hosts rows
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

`HostTooling` is `{ git: GitIdentity, github_cli: GithubCliStatus, ssh_agent: AgentStatus }`, and
each part carries a `ProbeOutcome` **before** its findings:

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

All three facts are per-user: `git config --global` reads `$HOME/.gitconfig`, `gh auth status`
reads `$HOME/.config/gh/hosts.yml`, and an agent socket belongs to one login. A probe run as the
daemon's own user answers for a different account, so the two commands go through
`spawner::start_output_as_user`, which resolves the user with `getpwnam_r`, sets
`HOME` and `PATH`, chdirs into that home, and drops privilege in `pre_exec`
(`setgid` → `initgroups` → `setuid`). `as_user_command` holds that setup once, shared with
`run_output_as_user`, because a second copy that drifted would produce a child running as the wrong
user or reading the wrong `$HOME` — precisely the answer these callers exist to get right.

The agent is scoped to the same user by a different route: nothing is spawned for it, so there is
no privilege to drop. The socket is located from the user's **uid** instead — see
[The ssh-agent](#the-ssh-agent) below.

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
`user.email`, `gh auth status`) and the agent conversation beside them are **started** before any is
collected, so the Hosts screen waits for the slowest rather than the sum, and a `gh` that reaches the
network cannot decide how long the git answer takes.

The agent is started **first and collected first**. It carries its own, shorter bound, and a struct
literal evaluates its fields in order — so collecting it after `gh`, which can reach the network,
would fail it for time `gh` had spent. Reaching `PROBE_TIMEOUT` on it therefore means it had the
whole deadline and never came back, which is `AgentStatus::failed` and never the empty key list an
operator would read as "this host has no keys loaded".

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

## The ssh-agent

`ssh_agent.rs` answers the third question: is an agent reachable for this host's OS user, and what
is it holding. `host_tooling.rs` starts it beside the two commands and puts its answer in the same
response.

### The wire protocol, not `ssh-add -l`

The daemon speaks `REQUEST_IDENTITIES` / `IDENTITIES_ANSWER` itself. `ssh-add -l`'s output is
human-readable text rather than an API, and the states an operator needs told apart are only
unambiguous on the wire:

| Outcome | `ssh-add -l` | protocol |
|---|---|---|
| agent holding keys | exit 0, parse the text | `IDENTITIES_ANSWER` with entries |
| agent, no keys | exit 1 and a message | `IDENTITIES_ANSWER`, empty |
| no agent reachable | exit 2 and a message | connect fails |
| `ssh-add` missing | spawn fails | n/a — no binary is involved |

The middle two are distinguished by an exit code and a message string alone, and getting that wrong
tells an operator their agent is empty when it is absent, or the reverse.

It also pays forward: loading a key means decrypting the private key **in process** and handing the
agent an `ADD_IDENTITY`, so the flow never needs a TTY or `SSH_ASKPASS`. Borrowing the encoding once
is why the crate is here rather than in the node that adds keys.

`ssh_agent_lib::blocking::Client` speaks the protocol; this module supplies the transport and every
bound. `fingerprint_of` and `key_type_of` are split out as pure functions of a public key blob, so
the derivation can be pinned against a fingerprint computed **outside this codebase**.

### Four outcomes, and the axis that separates two of them

`AgentStatus` is `{ outcome: ProbeOutcome, reachable: bool, keys: Vec<AgentKey> }`, and `reachable`
exists because two states otherwise arrive identically — both with an empty key list:

| State | `outcome` | `reachable` | `keys` |
|---|---|---|---|
| an agent answered, holding keys | `Ok` | `true` | the identities |
| an agent answered, holding none | `Ok` | `true` | empty |
| no agent answered | `Ok` | `false` | empty |
| nothing was established | `Failed(reason)` | `false` | empty |
| the platform has no such socket | `Unsupported` | `false` | empty |

An empty agent needs a key loaded; an absent one needs an agent started. Reporting either as the
other sends an operator to the wrong machine-level fix, so the two are separate fields rather than
one collapsed emptiness. Finding no agent is a **successful probe with a negative finding**, not a
failure.

`AgentKey` is `{ key_type, fingerprint, comment }` and deliberately carries **no originating path**:
the agent does not know which file an identity came from. The comment is free text set at
key-generation time and must never be rendered as a file location.

### Locating the socket

There is no way to *ask* for another user's `SSH_AUTH_SOCK`. It is set in that user's login session,
and `spawner::run_capture_as_user` builds a child environment rather than inheriting one, so the
daemon does not simply have the value to hand. The socket is located on the filesystem instead.

`AgentSocketResolver::socket_for(os_user) -> Result<Option<PathBuf>, String>` is the seam, and it
has **three** answers rather than two. `Ok(None)` is "I looked and this user has no agent"; `Err` is
"I could not look". They are separate evidence and send an operator to two different places — one to
start an agent, the other to their directory service — so collapsing them would report a failure as
a negative finding. A `getpwnam_r` that fails, which `pty_runtime::resolve_pty_os_user` returns for
both an unknown name and a directory service that did not answer, is the second.

`WellKnownAgentSockets` resolves the user's uid and then looks in exactly two places, and only ever
at a socket belonging to the user being asked about:

1. **The daemon's own `SSH_AUTH_SOCK`**, and only when the uid asked about is the daemon's own. It
   names one user's agent — ours — so it is an answer about nobody else. A stale value, an agent
   that has since exited, falls through rather than deciding the answer.
2. **The per-user runtime directory**, `/run/user/<uid>/`, at the three fixed names a user-session
   agent publishes: `ssh-agent.socket` (a systemd user unit), `gnupg/S.gpg-agent.ssh` (`gpg-agent`
   with `enable-ssh-support`) and `keyring/ssh` (`gnome-keyring-daemon`).

A shell-launched `ssh-agent` at `/tmp/ssh-XXXXXX/agent.<pid>` is deliberately **not** hunted for.
The directory name is random, a user with several login sessions has several of them, and picking
one would report one session's keys as the host's. "No agent reachable" is the honest answer where
the socket cannot be named.

### Reaching the socket on a supervised host

**Locating a socket is not the same as being allowed to open it**, and on the deployment shape
`./install --systemd` produces the daemon is usually not allowed to.

That install creates an unprivileged system account (`tddy` by default,
`useradd --system … --shell /usr/sbin/nologin`) and `tddy-supervisor` runs `tddy-daemon` as it. A
user-session agent's socket is reachable only by the user that owns it and by root: `/run/user/<uid>`
is `0700`, and a shell-launched agent's `/tmp/ssh-XXXXXX` directory is too. So a supervised daemon
can enumerate **its own service account's** agent — which normally has none — and no other host
user's.

**The constraint is filesystem permission, not an environment allowlist.** The supervisor's
`resolve_env` (`packages/tddy-supervisor/src/policy.rs`) is an allowlist whose own test fixture uses
`SSH_AUTH_SOCK` as the example of a denied key, and that is easy to mistake for the mechanism here.
It is not: `resolve_env` gates the environment a caller may put on a **session the daemon asks the
supervisor to spawn**, and says nothing about a socket the daemon opens in its own process. A
declared managed service — which is what the daemon is to the supervisor — is started with
`EnvironmentBase::Inherited`, the supervisor's own environment verbatim, so a `SSH_AUTH_SOCK` on the
supervisor unit does reach the daemon.

Under `./install --systemd --user` there is no supervisor and the daemon runs as the invoking user,
so that user's own agent is reachable in the ordinary way.

**What the probe does about it.** A candidate the daemon may not even `stat` is still handed back,
rather than skipped:

- `runtime_dir_agent_socket` remembers a candidate that failed with `PermissionDenied` and returns it
  when no readable socket was found, so the connect attempt happens and reports the real error.
- `WireProtocolAgentProbe::identities` treats **only** `ErrorKind::NotFound` and
  `ErrorKind::ConnectionRefused` from `UnixStream::connect` as "no agent reachable". Every other
  refusal — a permission error above all — is `Failed`, carrying the socket path and the error, and
  the row reads "could not check" with the reason.
- The one thing never produced is a **fabricated empty key list**. "No keys loaded" is only ever
  said by an agent that answered.

Skipping the unreadable candidate would have been the easy path and the wrong one: it would report
"no agent" for a host whose agent is running perfectly well, and send an operator to start one.

**What loading a key would need, and who decides it.** Making the add-key flow
(`#hosts-screen` 6/8) useful on a supervised host needs a **privileged path to the socket** — for
instance the supervisor connecting after `setuid` to the target user and passing the connected fd
back over the socket it already holds. That is a change to `tddy-supervisor`, not to this probe, and
it is a decision someone has to take rather than a gap this module can close. Recorded under
*Host tooling probe* in [`docs/dev/TODO.md`](../../../docs/dev/TODO.md).

### Which bytes each fact is taken from

Both facts are recovered by re-encoding the identity's credential, but from two different encodings
of it, because `ssh-add -l` draws its line between them exactly there:

- the **key type** from the credential as a whole, so a certificate is listed as a certificate
  rather than as the key inside it — the distinction OpenSSH prints as `(ED25519-CERT)`, and the row
  as `ssh-ed25519-cert-v01@openssh.com`;
- the **fingerprint** from `PublicCredential::key_data()`, the key being certified, because that is
  the one OpenSSH hashes for both. `ssh-keygen -lf id.pub` and `ssh-keygen -lf id-cert.pub` print
  the same digest, and hashing the certificate's own bytes would produce a value an operator could
  not match against anything they can print — which would defeat showing a fingerprint at all.

The key type is read out of the blob rather than mapped through a table of the types we know, so a
host holding a key this daemon has never heard of is still listed under the name its own agent uses.

### Bounding a blocking client

`ssh_agent_lib`'s client is blocking and takes no deadline of its own, so every bound lives on the
transport it is handed:

- a **read timeout and a write timeout** on the socket, which bound one read or one write; and
- **`UntilDeadline`**, a wrapper that fails the exchange once it has run past `AGENT_TIMEOUT` (3 s)
  in total. Without it an agent dribbling a byte at a time would renew the socket timeout for as
  long as it liked.

Both socket timeouts are armed **once, on the freshly connected socket**: macOS refuses `setsockopt`
on a socket whose peer has already answered and hung up, which is exactly where re-arming between
reads would land. That is why the whole-exchange bound is a deadline the transport checks rather
than a timeout it re-applies. The two together cap the exchange at twice `AGENT_TIMEOUT` — one read
may begin just inside the deadline and then take a full socket timeout of its own — which is why the
agent's worst case can outlast `PROBE_TIMEOUT` and why `host_tooling.rs` collects it first.

`SshAgentProbe::identities` is therefore synchronous and self-bounding, and it runs on the thread
`start_agent_probe` already starts for it. Nothing there starts a thread of its own and nothing
reaches into the async runtime's blocking pool — `HostToolingProbe::probe` is *itself* already
running inside a `spawn_blocking` task, so a nested one would block a pool thread on another pool
thread. Like the two commands beside it, an overrunning agent probe is abandoned rather than killed.

A timeout reaches the caller in two shapes — the client wraps its I/O errors in a protocol error on
the request/response path, while its other entry points produce `AgentError::IO` — and
`timed_out_talking` matches both. Matching one would report a timeout as an unreadable answer.

### Known limits

Carried here because none of them is visible from a passing test run:

- **One unreadable identity fails the whole list.** `Identity::decode_vec` decodes the answer
  all-or-nothing, so an agent holding a single key this version of `ssh-key` cannot parse is reported
  as a probe failure rather than as the host's other keys. A failure and never an empty list, so it
  cannot be read as "this host has no keys loaded" — but the other keys are not listed either.
- **A comment that is not UTF-8 sinks the list with it**, for the same reason: the crate decodes a
  comment as a `String`. Reading the field by hand rendered such a comment lossily and kept the key,
  so this is a behavioural regression from the switch to the library, not merely a coarser message.
- **No cap on the announced answer length.** `Client::handle` resizes its buffer to the length the
  peer announced before this module's transport sees a byte, so a socket that is not an agent can
  make the daemon reserve up to 4 GiB before the read fails. The mitigation is reachability, not a
  bound: `WellKnownAgentSockets` only ever hands back a socket owned by the user being asked about.
- **Finding the socket is not bounded at all.** `getpwnam_r` (through `resolve_pty_os_user`) and
  `UnixStream::connect` both run before any deadline exists and neither takes one. The bounds above
  cover the conversation, not getting to it — so a wedged socket or a wedged NSS backend parks one
  thread per poll per host, and the Hosts screen polls.
- **A certificate reports the certified key's fingerprint with the certificate's own key type.**
  That is what `ssh-add -l` shows, and it is stated here because the two halves of one row coming
  from two different blobs is otherwise a surprise.

## Failures are logged

`Failed` is the one outcome whose cause lives on the daemon's side of the wire — a missing tool, a
refused spawn, a timeout. `warn_if_failed` writes one `log::warn!` per failed part — git, `gh`,
ssh-agent — naming the OS user and the reason, because without it the only trace of a broken probe is a cell on a screen
nobody may be looking at.

## Non-Unix

`start_output_as_user` is Unix-only, and so is a Unix-domain agent socket, so `probe` on every
other target returns `ProbeOutcome::Unsupported` for all three parts. That is a property of the platform, not of the host's
tooling, and it says so rather than surfacing an internal error an operator would try to fix.

## Testing

| Level | Where | What only exists there |
|---|---|---|
| Unit | `host_tooling.rs` `#[cfg(test)]` | classification per outcome, and the timeout's kill-and-reap |
| Unit | `ssh_agent.rs` `#[cfg(test)]` | the protocol exchange, the four outcomes, fingerprint derivation |
| Integration | `connection_service.rs` `#[cfg(test)]` | auth rejection, the OS user the probe is handed, peer routing, the agent block reaching the wire whole |
| Component | `packages/tddy-web/cypress/component/HostsScreenToolingAcceptance.cy.tsx`, `HostsScreenSshAgentAcceptance.cy.tsx` | the states rendering distinguishably |

Shelling out to the real `git` / `gh` was rejected: CI has an arbitrary `gh` state, and a suite that
asserts against it is environment-dependent by construction. Talking to the developer's own
ssh-agent was rejected for the same reason.

**The agent harness is a real agent, not a mock.** `FakeAgent` binds an actual `UnixListener`,
reads the request frame and writes a correctly framed `IDENTITIES_ANSWER`. A mocked client would
exercise none of the encoding, and the encoding is the entire reason this module speaks the protocol
rather than `ssh-add`. It asserts the **whole** request frame, length prefix included: the request is
the library's to encode, so asserting the type byte alone would let its framing drift unnoticed.

`SilentAgent` covers the hang. It accepts the connection and hands the accepted stream **out** of the
accepting thread to be held by the fixture — a dropped `UnixStream` closes the socket, and the probe
would then see a peer that hung up, which is a different failure and one that returns immediately.
The test asserts `elapsed >= AGENT_TIMEOUT` as well as the failure, so it cannot regress to the
peer-closed path and still pass.

The fingerprint fixtures — a published ed25519 blob and an OpenSSH certificate over it — are pinned
against **`ssh-keygen -lf`'s own output**, not against this code's, so the assertions prove agreement
with OpenSSH rather than self-consistency.

`ProbeOutcome::Failed` carries its reason as a `String` for an operator to read, not for a caller to
parse. Nothing branches on the text.

## See also

- [connection-service.md](./connection-service.md) — the `GetHostTooling` RPC and its routing
- [`docs/dev/TODO.md`](../../../docs/dev/TODO.md) § *Host tooling probe* — the open deployment
  questions this probe surfaced
- `host_stats.rs` — the other per-host reader, documented in
  [connection-service.md § Host stats](./connection-service.md#host-stats); this module copies its
  injectable-trait shape
- Feature: [docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)
- Web: [packages/tddy-web/docs/hosts-screen.md](../../tddy-web/docs/hosts-screen.md)
