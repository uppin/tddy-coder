# 2026-09-06 — Reading a host user's ssh-agent over the wire protocol
**Type:** Feature

`ssh_agent.rs` is the first place tddy speaks SSH at all. `SshAgentProbe::identities(os_user)` returns
an `AgentStatus` — `{ outcome, reachable, keys }` — which `host_tooling.rs` puts on the
`GetHostTooling` response beside the git and `gh` findings.

`ssh_agent_lib::blocking::Client` speaks the protocol; this module supplies the transport and every
bound. Choosing the protocol over `ssh-add -l` is what makes the four outcomes unambiguous: on the
wire, an agent holding nothing answers with an empty `IDENTITIES_ANSWER` while an absent one fails to
connect, where `ssh-add` separates them by an exit code and a message string.

**`reachable` is a field because two states otherwise arrive identically.** An empty agent and an
absent one both carry no keys, and they send an operator to two different fixes. Likewise
`AgentSocketResolver::socket_for` returns `Result<Option<PathBuf>, String>` and not
`Option<PathBuf>`: `Ok(None)` is "I looked and found none", `Err` is "I could not look" — a
`getpwnam_r` that fails establishes nothing about a user's keys, and reporting it as "no agent" would
send someone to start one that is already running.

**Locating the socket is the node's design problem.** There is no way to ask for another user's
`SSH_AUTH_SOCK` — it is set in that user's login session, and `run_capture_as_user` builds a child
environment rather than inheriting one. `WellKnownAgentSockets` resolves the user's uid and looks in
two places: the daemon's **own** `SSH_AUTH_SOCK`, and only when the uid asked about is the daemon's
own; then `/run/user/<uid>` at the three fixed names a systemd user unit, `gpg-agent` and
`gnome-keyring` publish. A shell-launched agent under `/tmp/ssh-XXXXXX` is deliberately not hunted
for — the directory name is random and a user with several login sessions has several of them, so
picking one would report one session's keys as the host's.

**Reaching that socket is a separate question, and on a supervised host the answer is usually no.**
`./install --systemd` runs the daemon as an unprivileged service account; `/run/user/<uid>` is
`0700`, so the daemon can open only its own account's agent. This is a **filesystem permission**
limit and not the supervisor's `resolve_env` allowlist — that gates the environment a caller may put
on a session the daemon asks the supervisor to spawn, and a declared managed service like the daemon
is started with `EnvironmentBase::Inherited` anyway. The probe reports it as what it is: a candidate
that fails `stat` with `PermissionDenied` is still returned, so `connect()` surfaces the real error
and the outcome is `Failed` with it; **only** `ErrorKind::NotFound` and `ConnectionRefused` become
the "no agent" finding. Loading a key on such a host needs a privileged path to the socket — a
`tddy-supervisor` change, recorded in [`docs/dev/TODO.md`](../../../../docs/dev/TODO.md).

**The blocking client is bounded, not trusted.** Both socket timeouts are armed once on the freshly
connected socket — macOS refuses `setsockopt` after the peer has answered and hung up — and an
`UntilDeadline` transport wrapper fails the exchange once it has run past `AGENT_TIMEOUT` in total,
so an agent dribbling a byte at a time cannot renew the per-read timeout. The client runs on the
thread `host_tooling::start_agent_probe` already starts for it; nothing starts one of its own, and
nothing nests a `spawn_blocking` inside the one `probe` is already running in. Because the agent's
worst case is *twice* `AGENT_TIMEOUT`, it is started first and collected first, so it is never
failed for time `gh` spent on the network.

**Two facts from two encodings of one credential**, because `ssh-add -l` draws its line there: the
key type from the whole credential, so a certificate reads as a certificate; the fingerprint from
`PublicCredential::key_data()`, the key being certified, which is the digest OpenSSH prints for a key
and for a certificate over it alike. Both fixtures are pinned against `ssh-keygen -lf` rather than
against this code's own output.

**The test harness is a real agent.** `FakeAgent` binds an actual `UnixListener` and asserts the
whole request frame, length prefix included — the encoding is the library's, and asserting the type
byte alone would let its framing drift. `SilentAgent` hands the accepted stream out of its accepting
thread so the connection stays open; the timeout test asserts `elapsed >= AGENT_TIMEOUT`, so it
cannot pass down the peer-closed path in no time at all.

**Known limits, recorded in the module header and in
[`host-tooling-probe.md`](../host-tooling-probe.md):** no cap on the answer length the peer
announces; one undecodable identity fails the whole list; a non-UTF-8 comment sinks the list with it
(a behavioural regression from reading the field by hand, which rendered it lossily and kept the
key); and neither `getpwnam_r` nor `UnixStream::connect` is bounded, so a wedged socket or NSS
backend parks one thread per poll per host.

**Dependencies**, consented and pinned: `ssh-agent-lib 0.6.0` with `default-features = false` — its
`default = ["agent"]` is the *server* side, and turning it off drops `service-binding` and `raunch`
from `Cargo.lock` entirely — and `ssh-key 0.6.7`. `ssh-key`'s `encryption` feature stays off;
**`rsa` is on regardless of our own entry**, because `ssh-agent-lib` enables `ssh-key/crypto` and
feature unification applies it to ours. It cannot be turned off while depending on `ssh-agent-lib`.

Node 5 of the `#hosts-screen` stack — [PR #457](https://github.com/uppin/tddy-coder/pull/457).

See [`host-tooling-probe.md`](../host-tooling-probe.md) and
[`connection-service.md`](../connection-service.md).
