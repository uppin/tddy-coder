# Host tooling probe (tddy-daemon)

What a host has installed and configured, as opposed to how busy it is.

`host_stats.rs` reports load. `host_tooling.rs` reports the facts that decide whether work on a host
will actually succeed: the git identity its commits would carry, whether the GitHub CLI there is
authenticated, whether an ssh-agent is holding a key its remotes would accept, and whether a desktop
on it can be reached — and bridged. It backs the web's Hosts rows
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

`HostTooling` is
`{ git: GitIdentity, github_cli: GithubCliStatus, ssh_agent: AgentStatus, remote_desktop: Vec<DesktopReachability> }`,
and each part carries a `ProbeOutcome` **before** its findings:

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

Three of the four facts are per-user: `git config --global` reads `$HOME/.gitconfig`, `gh auth
status` reads `$HOME/.config/gh/hosts.yml`, and an agent socket belongs to one login. A probe run as the
daemon's own user answers for a different account, so the two commands go through
`spawner::start_output_as_user`, which resolves the user with `getpwnam_r`, sets
`HOME` and `PATH`, chdirs into that home, and drops privilege in `pre_exec`
(`setgid` → `initgroups` → `setuid`). `as_user_command` holds that setup once, shared with
`run_output_as_user`, because a second copy that drifted would produce a child running as the wrong
user or reading the wrong `$HOME` — precisely the answer these callers exist to get right.

The agent is scoped to the same user by a different route: nothing is spawned for it, so there is
no privilege to drop. The socket is located from the user's **uid** instead — see
[The ssh-agent](#the-ssh-agent) below.

The desktop readings are the one part of the answer that is scoped to **no** user: a desktop is
served by the host, not by one of its accounts, and the bridge binary belongs to the daemon rather
than to whoever is logged in. Nothing there spawns anything, so `os_user` never reaches it — see
[Remote desktop](#remote-desktop) below.

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
`user.email`, `gh auth status`), the agent conversation and the desktop connects beside them are all
**started** before any is collected, so the Hosts screen waits for the slowest rather than the sum,
and a `gh` that reaches the network cannot decide how long the git answer takes.

The agent is started **first and collected first**. It carries its own, shorter bound, and a struct
literal evaluates its fields in order — so collecting it after `gh`, which can reach the network,
would fail it for time `gh` had spent. Reaching `PROBE_TIMEOUT` on it therefore means it had the
whole deadline and never came back, which is `AgentStatus::failed` and never the empty key list an
operator would read as "this host has no keys loaded".

The desktop readings are collected next, and for the same reason: both self-bounding probes are
gathered into locals **before** the `HostTooling` literal is built, rather than sitting in it as
fields. See [Field order is the deadline](#field-order-is-the-deadline).

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

## Remote desktop

`remote_desktop_probe.rs` answers the fourth question, and it is really two questions that only look
like one:

| Fact | Question it answers | How |
|---|---|---|
| `can_bridge` | can tddy stream a desktop from this host **at all**? | the resolved bridge binary exists |
| `desktop_reachable` | is anything **serving** a desktop here? | a bounded TCP connect |

They are independent, and both are answered on every reading — a host that cannot bridge cannot
bridge whether or not a desktop is up. Collapsing them into one "available" flag would send an
operator to the wrong fix: "install the bridge" and "start a VNC server" are unrelated problems with
nothing in common but the row they would share.

`DesktopReachability` is `{ outcome, protocol, can_bridge, desktop_reachable, port }`, and
`PROBED_PROTOCOLS` is both of them — `Vnc` on **5900**, `Rdp` on **3389**. Both are reported for
every host, including the ones nothing answered for, because "no desktop on :5900" is a finding and a
block listing only the protocols that answered could not be told apart from one where nobody looked.

**The checked port always travels with the answer.** Port discovery is out of scope, so an
unreachable reading is only ever a statement about the port named beside it; without that port,
"unavailable" reads as authoritative for a host that simply serves on a display other than `:0`.

### A connect, and not one byte more

`is_accepting_connections(host, port)` opens a `TcpStream` with `connect_timeout` and drops it. That
is the whole interaction: **nothing is written**, so no protocol handshake is ever begun. A
monitoring screen polling half-open RFB handshakes against people's desktops on a timer is
antisocial, and the connect already answers the question being asked. The rudeness guard is a test
rather than this paragraph — see [Testing](#testing).

`CONNECT_TIMEOUT` is **750 ms**, deliberately far inside `PROBE_TIMEOUT`. The Hosts screen probes
every host it lists, for both protocols, on every poll, and an address that black-holes packets must
not be able to hold the tooling RPC open.

The address is always this daemon's own loopback (`PROBE_HOST`, `127.0.0.1`). Each daemon reports for
the machine it runs on, so a desktop served on another host's loopback is that daemon's reading to
take rather than this one's — which is what makes `GetHostTooling`'s existing peer routing the only
fan-out this block needs.

### A refusal is a finding; anything else is a failure

The classification is one `match`, and it is the point of the module:

- `Ok(_)` — something accepted the connection. `desktop_reachable: true`.
- `Err` with `ErrorKind::ConnectionRefused` — we reached the host and nothing is listening there.
  That is an **answer**: `Ok(false)`, and the reading is `ProbeOutcome::Ok` with
  `desktop_reachable: false`.
- **every other `Err`** — a timeout, an unreachable network, a permission denial — means the probe
  got no answer at all. It becomes `ProbeOutcome::Failed(reason)`, with `desktop_reachable: false`
  because a flag has to say something and `false` is the neutral value.

So one protocol on one host has four distinguishable states, and the outcome is the only thing that
separates "nothing is serving" from "we could not check":

| State | `outcome` | `can_bridge` | `desktop_reachable` |
|---|---|---|---|
| bridge present, desktop serving | `Ok` | `true` | `true` |
| bridge present, nothing serving | `Ok` | `true` | `false` |
| desktop serving, no bridge here | `Ok` | `false` | `true` |
| we could not check | `Failed(reason)` | as found | `false` |

**"We checked and the answer is no" is not "we could not check."** A reader keying off
`desktop_reachable` alone would report "No desktop" for a host it never reached, which is the same
class of fabricated fact as reporting an authenticated `gh` as logged out. That is why the outcome
travels with every reading rather than being inferred from the flags, and it is the same rule
`classify_gh_auth_status` follows one section above.

### The bridge check is the cheaper win

`resolve_vnc_binary_path` / `resolve_rdp_binary_path` (`crate::config`) resolve a path by *guessing*
— explicit config, then a sibling of `current_exe()`, then a bare name on `PATH` — with **no
existence check anywhere**. A missing binary surfaces only as a spawn error in
`screen_sharing_service`, i.e. after an operator has already asked for a stream. Checking it up front
turns that error into a fact on the row, and it costs a `stat`.

`binary_is_present` splits on whether the resolution has a directory in it:

- a path **with** a directory is `is_file()`-ed directly;
- a **bare name** — the resolvers' last resort, which they hand to the OS to look up on `PATH` — is
  searched along `PATH` too.

The second half is the whole reason the function exists. `Path::new("tddy-vnc").exists()` answers
about the **daemon's working directory**, which is not where the OS would find it — and under
`./install --systemd` there is no `WorkingDirectory=` at all, so that question is about `/`. It would
report "cannot bridge" for a bridge that is genuinely installed: the same conflation this module
exists to prevent, pointing the other way.

Resolution **order** is untouched. This module only asks whether what the resolvers return is there.

### It checks the path this daemon would really spawn

`TcpRemoteDesktopProbe` carries a `DaemonConfig`, and `bridge_binary_is_present` is a method on it,
because an existence check is worth no more than the path it checks. An operator who sets
`screen_sharing.vnc_binary_path` must have **that** path tested; answering from
`DaemonConfig::default()` would report a configured, installed bridge as absent.

The configuration therefore reaches it by construction, not by lookup:
`SubprocessHostToolingProbe::for_config(&config)` builds the probe from the daemon's own
configuration, and `connection_service.rs` hands it the live `config` it already has in scope — the
same value `LocalOnlyEligibleDaemonSource::for_config` is built from a few lines earlier. `Default` still
resolves against `DaemonConfig::default()`, which is exactly what a daemon with no `screen_sharing:`
block spawns.

### Field order is the deadline

`start_desktop_probes` starts one connect per protocol on a thread of its own and waits for none of
them; `desktop_readings_of` then waits on each channel until the shared deadline. A reading that
never arrives becomes `desktop_probe_failed` — `Failed`, naming the protocol and the port, and never
the "nothing is serving here" finding an operator would act on. Since a connect bounds itself at
`CONNECT_TIMEOUT`, reaching `PROBE_TIMEOUT` here means the reading did not come back at all.

The readings are then collected into a local **before** the `HostTooling` literal is built, beside
`ssh_agent` and for the identical reason: **a struct literal evaluates its fields in order.** With
`remote_desktop` sitting last in the literal, two bounded connects would be judged on whatever `git`
and `gh` had left of the shared deadline — and `gh` can reach the network. The block would then
report a host as unprobeable for time another probe spent, which is the one thing it must never
claim.

### On the wire

`HostRemoteDesktop` (`connection.proto`) carries `{ outcome, protocol, can_bridge,
desktop_reachable, port, failure_reason }`, and `GetHostToolingResponse.remote_desktop` is
`repeated` — one entry per probed protocol, beside the git, `gh` and ssh-agent blocks rather than on
an RPC of its own.

`protocol` is an `int32` holding **`screen_sharing.proto`'s `Protocol` values** (`1` = VNC, `2` =
RDP), which `DesktopProtocol`'s discriminants mirror rather than restate. A second enum meaning the
same thing is how two enums drift apart, and this one would have to agree with the service that
actually spawns the bridges.

`host_remote_desktop_message` is the one conversion, and it keeps `can_bridge` and
`desktop_reachable` as two fields for the reason they are two facts.

### What this block does not do

It reports. It starts no stream, spawns no bridge, opens no viewer, and creates no target of either
scope — `ScreenSharingService`, its per-session targets and its vault are untouched, and its
resolution order along with them. Opening a host's desktop is a separate action on
`ScreenSharingService`, and the target it needs is created there; see
[host-registry.md § Host-scoped desktop targets](./host-registry.md#host-scoped-desktop-targets).

**Known cost.** Probing on every screen refresh is a TCP connect per host per protocol. If a fleet
makes that noticeable, the fix is a short-TTL cache in front of the block; it is recorded rather than
pre-optimised.

## Failures are logged

`Failed` is the one outcome whose cause lives on the daemon's side of the wire — a missing tool, a
refused spawn, a timeout. `warn_if_failed` writes one `log::warn!` per failed part — git, `gh`,
ssh-agent — naming the OS user and the reason, because without it the only trace of a broken probe is a cell on a screen
nobody may be looking at.

`warn_if_a_desktop_probe_failed` does the same for the desktop block, once per failed reading and
naming the protocol and the port instead of an OS user, because a desktop is not one account's. It
is a separate function rather than a fourth call to `warn_if_failed` because that one takes the OS
user it names, and passing it a user the reading was never scoped to would put a name in the log
that means nothing.

## Non-Unix

`start_output_as_user` is Unix-only, and so is a Unix-domain agent socket, so `probe` on every other
target returns `ProbeOutcome::Unsupported` for git, `gh` and the agent. That is a property of the platform, not of the host's
tooling, and it says so rather than surfacing an internal error an operator would try to fix.

**The desktop block is the exception, and the `#[cfg(not(unix))]` arm probes it for real.** Nothing
it does needs `start_output_as_user`: it is a TCP connect to this machine's own loopback plus an
existence check on a path, and both answer on every platform. `Unsupported` for a probe that would
have answered is as much a fabricated fact as a finding nobody made, so that arm starts the same
`start_desktop_probes` and collects it through the same `desktop_readings_of` against the same
`PROBE_TIMEOUT`, and calls `warn_if_a_desktop_probe_failed` afterwards like the Unix one. The code
says so where a reader would otherwise assume symmetry with the three fields above it.

That asymmetry is what `assert_was_probed_on_this_host` guards from the other side: the Unix tests
assert git, `gh` and the agent are **not** `Unsupported` on a Unix host, since that is the one answer
a Unix host can never honestly give for them.

## Testing

| Level | Where | What only exists there |
|---|---|---|
| Unit | `host_tooling.rs` `#[cfg(test)]` | classification per outcome, and the timeout's kill-and-reap |
| Unit | `ssh_agent.rs` `#[cfg(test)]` | the protocol exchange, the four outcomes, fingerprint derivation |
| Unit | `remote_desktop_probe.rs` `#[cfg(test)]` | a real socket's behaviour: reachable, refused, no bytes written, and the bridge check against a configured path |
| Integration | `host_tooling.rs` `#[cfg(test)]` | the desktop block arriving **beside** the other three rather than instead of them, and the two facts staying apart |
| Integration | `connection_service.rs` `#[cfg(test)]` | auth rejection, the OS user the probe is handed, peer routing, the agent block reaching the wire whole |
| Component | `packages/tddy-web/cypress/component/HostsScreenToolingAcceptance.cy.tsx`, `HostsScreenSshAgentAcceptance.cy.tsx`, `HostsScreenRemoteDesktopAcceptance.cy.tsx` | the states rendering distinguishably |

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

**The desktop probe's harness is a real socket too.** `a_listener` binds `127.0.0.1:0` and
`a_closed_port` binds one and drops it, so the reachable and the refused case are the kernel's own
answers on an ephemeral loopback port — hermetic, needing no network, and unable to collide with a
service the developer happens to be running. That matters because [the CI gate](../../../docs/dev/guides/ci.md)
deliberately excludes the VM-backed and desktop suites, so anything here that reached a real desktop
would be flaky where it ran at all.

`writes_no_bytes_to_the_remote_before_closing` is the one test that could not be replaced by a
mock: the listener **records what it received** and the assertion is that zero bytes arrived.
"We do not handshake" is otherwise a comment rather than a fact.

`reports_that_the_host_can_bridge_when_the_configured_binary_exists` writes a file into a `tempfile`
directory and names it in a `DaemonConfig`, so the reading is about the configured path and never
about what the machine running the suite has installed — the same rule the git and `gh` fakes follow.
Its mirror, `reports_that_the_host_cannot_bridge_when_the_binary_is_missing`, asserts a **reachable**
desktop with **no** bridge in the same reading, which is the pair of facts a single flag could not
carry.

`host_tooling.rs`'s two integration tests inject a scripted `RemoteDesktopProbe` through
`SubprocessHostToolingProbe::probing_desktops_with`, because what a connect finds depends on what is
listening on the machine running the suite and a test may not depend on that. The seam is on
`SubprocessHostToolingProbe` and not a second builder override on the service:
`with_host_tooling` already substitutes the whole `HostTooling`, `remote_desktop` included, so a
parallel `with_remote_desktop_probe` would have had no caller — and dead surface is worse than a
missing one.

`ProbeOutcome::Failed` carries its reason as a `String` for an operator to read, not for a caller to
parse. Nothing branches on the text.

## See also

- [connection-service.md](./connection-service.md) — the `GetHostTooling` RPC and its routing
- [host-add-key.md](./host-add-key.md) — the agent's **write** side: loading a key into it, and the
  socket resolution this module owns being called rather than reimplemented
- [`docs/dev/TODO.md`](../../../docs/dev/TODO.md) § *Host tooling probe* — the open deployment
  questions this probe surfaced
- `host_stats.rs` — the other per-host reader, documented in
  [connection-service.md § Host stats](./connection-service.md#host-stats); this module copies its
  injectable-trait shape
- Feature: [docs/ft/web/hosts-screen-tooling.md](../../../docs/ft/web/hosts-screen-tooling.md)
- Web: [packages/tddy-web/docs/hosts-screen.md](../../tddy-web/docs/hosts-screen.md)
- Feature: [docs/ft/web/screen-sharing-sessions.md](../../../docs/ft/web/screen-sharing-sessions.md)
  — the per-session bridging whose binaries the desktop block checks for
