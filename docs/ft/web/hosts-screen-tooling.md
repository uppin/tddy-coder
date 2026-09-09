# Host tooling on the Hosts screen

Every row of the Hosts screen reports what that host has installed and configured: the **git
identity** its commits would carry, the state of the **GitHub CLI** on it, and the **ssh-agent** it
has, with the keys that agent is holding.

## Motivation

Nowhere in tddy says what a host will commit as. A session runs on a machine, makes commits there
and pushes them from there, and all three depend on facts that live in that machine's own home
directory — `~/.gitconfig`, `~/.config/gh/hosts.yml`, and the agent that user's login session
started. When a host commits under the wrong identity, or its `gh` is logged out so a push fails, or
its agent holds no key its remotes accept, the operator finds out from the result rather than from
the fleet list. A clone or push that fails for want of a usable key reports a generic git error and
nothing about why.

This is the first *capability probe* in tddy. The telemetry on the same row says how busy a machine
is; this says whether work on it will succeed.

## What a row shows

Three sections, each labelled with the tool it speaks for.

| Cell | State | Reading |
|---|---|---|
| git | configured | the `user.name` and `user.email` commits on that host would carry |
| git | none configured | "Not configured" — the probe ran and the host has no identity set |
| git | could not check | the probe did not run, or its answer was not understood |
| gh | authenticated | the login `gh` there is authenticated as |
| gh | installed, logged out | "Not authenticated" |
| gh | not installed | "Not installed" — `gh` is not on that host's `PATH` |
| ssh-agent | holding keys | one line per key: its type, its whole fingerprint, and its comment |
| ssh-agent | agent, no keys | "No keys loaded" — an agent is running and holds nothing |
| ssh-agent | no agent | "No agent" — nothing answered for that host's OS user |
| ssh-agent | could not check | the probe failed, with the reason on hover |

A host that has not answered yet renders a waiting marker, and a platform that cannot run the probes
at all says so. Neither borrows the shape of a finding.

## Absence is reported, never blanked

Every one of these states sends an operator somewhere different, so none of them collapses into an
empty string or into another. Three distinctions carry the whole design:

- **"Could not check" is not "not configured".** One is a host to go and fix; the other is a probe
  to go and fix. A failed probe rendered as a negative finding sends an operator to set an identity
  that is already set.
- **Unrecognised output is a failure, not a finding.** `gh auth status` has no stable output
  contract, and reporting an authenticated host as logged out is the worse error of the two.
- **"No keys loaded" is never said about an agent that did not answer.** Only an agent that
  answered can report an empty list; every other emptiness is "No agent" or "Could not check". An
  empty agent wants a key added, an absent one wants an agent started, and a failed probe wants
  looking at on the daemon side.

A half-configured host — a `user.name` with no `user.email`, or the reverse — reports "not
configured". Git refuses to commit without both, so such a host has no identity its commits would
carry, and showing the half it does have beside a blank would state something untrue.

## The `gh` login is the host's

Three GitHub identities exist in tddy at once, and they can all disagree:

| Identity | Where it lives | Shown as |
|---|---|---|
| The tddy session's user | the operator's browser session | the avatar in the app shell (`UserAvatar`) |
| A `GITHUB_TOKEN` / `GH_TOKEN` | a host's environment, or a session's | nowhere |
| The host's `gh` login | that host's `~/.config/gh/hosts.yml` | this row's `gh` cell |

Only the last is what this row reports. It is labelled as the tool it comes from, and hovering the
cell names the host explicitly, because an unlabelled login sitting next to the operator's own
avatar reads as whichever identity the reader expected to see there.

## What a key row can honestly say

An agent reports, per identity, a public key blob and a comment. From those a row shows the **key
type**, the **`SHA256:` fingerprint** `ssh-add -l` prints, and the **comment**.

**A key's originating file is not shown, because the agent does not know it.** The comment is
arbitrary free text set when the key was generated — commonly `user@host`, often a path, sometimes
neither — and presenting it as a file location would state a fact nobody established. It is rendered
as what it is.

**The fingerprint is rendered whole.** Two keys can share any prefix of one, so a shortened
fingerprint identifies nothing an operator could line up against their own `ssh-add -l`.

A certificate is listed under its own type (`ssh-ed25519-cert-v01@openssh.com`) with the fingerprint
of the key it certifies — which is what `ssh-add -l` shows for one, so the row and the operator's own
terminal agree.

## What a supervised host can report

On a host installed with `./install --systemd`, the daemon runs as an unprivileged service account.
A user-session agent's socket is reachable only by the user that owns it and by root, so such a
daemon can enumerate **its own account's** agent and no other host user's. Those rows read "could not
check", with the permission error on hover — never "No agent", and never an empty key list.

A daemon installed with `--user`, or run as the operator's own account, reports that account's agent
in the ordinary way.

Loading a key onto such a host runs into the same wall, and for the same reason: the add speaks to
the socket the read side resolved. It needs a privileged path to it, which is a change to
`tddy-supervisor` rather than to this screen. See
[`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md#reaching-the-socket-on-a-supervised-host).

## Reporting, and the one action

The probes themselves change nothing. There is no "configure git" and no "log in" action, and no
general "run this on host X" primitive exists — the daemon runs three fixed probes and nothing else.

The ssh-agent section carries the screen's **only** write action: **loading a key into that host's
agent**, offered on a row whose agent answered, and only there. A host whose agent did not answer
needs an agent started, not a key loaded. The passphrase for that key is asked for by the host,
answered in the browser, and carried back encrypted under that host's own public key; it is never
persisted, logged or written to disk. That flow is its own feature —
[hosts-screen-add-key.md](./hosts-screen-add-key.md) — and nothing else on the row writes anything.

Removing a key from an agent, and generating one, are not offered.

## Relationship to the rest of the screen

The tooling cells sit on the same row as the [per-host telemetry](./hosts-screen-telemetry.md) and
the identity columns from [`hosts-screen.md`](./hosts-screen.md). Unlike telemetry, tooling is not
streamed: what is installed on a machine changes on a human timescale, so it is asked for rather
than subscribed to, and only for hosts that are online.

## Acceptance criteria

- [x] A host with a configured git identity shows its `user.name` and `user.email`.
- [x] A host with no git identity reads "not configured", not an empty name.
- [x] A host without `gh` reads "not installed".
- [x] A host whose `gh` is installed but logged out reads "not authenticated".
- [x] A host whose `gh` is authenticated shows the login it is authenticated as.
- [x] A probe that fails or times out reads as a probe failure, distinct from every state above.
- [x] The probes read the host's **own** OS user's configuration.
- [x] A request naming another host is answered by that host.
- [x] The `gh` cell identifies its login as the host's, distinguishably from the tddy session user.
- [x] A host with a reachable agent holding keys lists each key's type, fingerprint and comment.
- [x] A host with a reachable agent holding no keys reads "No keys loaded", distinct from having no
      agent.
- [x] A host with no reachable agent reads "No agent", distinctly.
- [x] An agent that does not answer within the timeout reads as a probe failure, distinct from every
      state above.
- [x] Fingerprints are shown in the `SHA256:` form `ssh-add -l` displays.
- [x] A key's comment is presented as a comment, never as a file path.
- [x] The agent is resolved for the host's **own** OS user, matching the git and `gh` probes.
- [x] The row renders each fingerprint whole, never truncated into ambiguity.

## Not yet reachable in the running app

⚠ **No screen mounts these cells.** `HostRowTooling` — and with it the ssh-agent section — is not
rendered by `HostsScreen` or any other component in `packages/tddy-web/src`; nothing there issues
`GetHostTooling` either. Its only call sites are the Cypress component specs, which mount it
directly.

So everything above is implemented, tested and reachable over the wire, and **an operator cannot see
any of it yet**. Mounting the section on the Hosts row belongs to the node that owns that row. Until
then this page describes a contract rather than a screen someone can open, and the assembled path
— row → RPC → probe → cells — has never run.

## Technical reference

- Daemon: [`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md),
  [`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md)
- Web: [`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md)
- Feature: [hosts-screen-add-key.md](./hosts-screen-add-key.md) — loading a key into the agent this
  section reports on
