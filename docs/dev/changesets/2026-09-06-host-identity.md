# 2026-09-06 — Each host's git identity and GitHub CLI status
**Type:** Feature

Spans `tddy-service` (the `GetHostTooling` RPC and its messages), `tddy-daemon` (`host_tooling.rs`,
the handler, the spawner's new start-and-keep-the-handle path) and `tddy-web` (`HostRowTooling`).

The first **capability probe** in tddy: nodes 1–3 of the `#hosts-screen` stack report facts the
daemon already holds, this one asks what is installed and configured on the machine. It owns the RPC
shape the ssh-agent (node 5) and remote-desktop (node 7) probes extend, which is why it is one RPC
that grows blocks rather than three near-identical ones.

The design-bearing rule is that **absence is reported, not blanked**. Six outcomes stay distinct end
to end — git configured / none configured / probe failed, and `gh` not installed / logged out /
authenticated as a login — because each sends an operator somewhere different and an empty string in
place of any of them is a fabricated fact. Its sharpest form: output `gh auth status` did not produce
in a shape the daemon recognises is a **probe failure**, never "logged out". Reporting an
authenticated host as logged out is the worse error.

Two fixed probes, deliberately, rather than a session-less "run this on host X" RPC — that primitive
would be the largest security surface in the stack and nothing here needs it. The node reads only:
no `git config --set`, no `gh auth login`.

**Deferred at wrap**, and recorded under *Host tooling probe* in [`docs/dev/TODO.md`](../TODO.md)
rather than in a working document: an unprivileged daemon cannot probe another OS user, the probe
has no cache or in-flight dedup ahead of a UI that polls every host, and no code path issues
`GetHostTooling` yet — `HostRowTooling` is unmounted, and node 1 owns the row.

Node 4 of the `#hosts-screen` stack — [PR #456](https://github.com/uppin/tddy-coder/pull/456).

See [`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md),
[`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md) and
[`docs/ft/web/hosts-screen-tooling.md`](../../ft/web/hosts-screen-tooling.md).
