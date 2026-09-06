# 2026-09-06 — Each host's ssh-agent and the keys it holds
**Type:** Feature

Spans `tddy-service` (an `ssh_agent` block on the `GetHostTooling` response), `tddy-daemon`
(`ssh_agent.rs`, the socket resolver, the probe wiring in `host_tooling.rs`) and `tddy-web`
(`HostRowSshAgent`). Node 5 of the `#hosts-screen` stack —
[PR #457](https://github.com/uppin/tddy-coder/pull/457).

Nothing in tddy had ever spoken to an ssh-agent: `Cargo.lock` contained no SSH crate at all and
nothing read `SSH_AUTH_SOCK`. So when a session failed to clone or push for want of a usable key,
tddy reported a generic git error and nothing about why.

The daemon speaks the **agent wire protocol** rather than parsing `ssh-add -l`. Its output is
human-readable text, not an API, and "no agent" versus "an agent holding nothing" is distinguished
there by an exit code and a message string — the two states an operator most needs told apart, since
one wants an agent started and the other wants a key loaded. Read-only throughout: no key is added,
removed or unlocked, and no passphrase is asked for or carried anywhere.

**The node's declared release blocker had an answer, and it is not the one the plan expected.**
Whether a supervised daemon can reach an agent socket at all was called a blocker rather than a
detail, because it decides whether this node is useful in production. It is a **filesystem
permission** limit: `./install --systemd` runs the daemon as an unprivileged service account, and a
user-session agent's socket lives under a `0700` `/run/user/<uid>` (or a `0700` `/tmp/ssh-XXXXXX`),
so such a daemon can enumerate only its own account's agent. It is *not* the supervisor's
`resolve_env` allowlist, which the planning documents blamed: that gates the environment the daemon
requests for a **session it spawns** and has nothing to do with a socket the daemon opens in
process — a declared managed service is started with `EnvironmentBase::Inherited`. The code reports
the limit honestly rather than hiding it: a candidate that cannot be `stat`ed for `EACCES` is still
handed back so `connect()` surfaces the permission error and the row reads "could not check" with
the reason, and only `ENOENT` / `ECONNREFUSED` produce the "no agent" finding. Never a fabricated
empty key list.

**Deferred at wrap**, and recorded under *Host tooling probe* in [`docs/dev/TODO.md`](../TODO.md)
rather than in a working document: loading a key on a supervised host needs a privileged path to the
socket (the supervisor connecting after `setuid` and passing the fd back, say) — a `tddy-supervisor`
change and a decision someone has to take; and `ssh_agent.rs`'s known limits, none of which a
passing test run makes visible. The unmounted-row limit node 4 recorded still stands: nothing in
`packages/tddy-web/src` renders `HostRowTooling` or issues `GetHostTooling`, so this node's daemon
work is not yet observable in the running app.

**Two external dependencies** enter the workspace, consented under CLAUDE.md § ASK and pinned:
`ssh-agent-lib 0.6.0` (`default-features = false` — its `default = ["agent"]` is the *server* side)
and `ssh-key 0.6.7`. The key crate is unused by this node's behaviour and lands here deliberately, so
the add-key node inherits a settled dependency set instead of introducing crates and the riskiest
flow in the stack at once.

See [`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md),
[`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md) and
[`docs/ft/web/hosts-screen-tooling.md`](../../ft/web/hosts-screen-tooling.md).
