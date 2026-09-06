# Host tooling on the Hosts screen

Every row of the Hosts screen reports what that host has installed and configured: the **git
identity** its commits would carry, and the state of the **GitHub CLI** on it.

## Motivation

Nowhere in tddy says what a host will commit as. A session runs on a machine, makes commits there
and pushes them from there, and all three depend on facts that live in that machine's own home
directory — `~/.gitconfig` and `~/.config/gh/hosts.yml`. When a host commits under the wrong
identity, or its `gh` is logged out so a push fails, the operator finds out from the result rather
than from the fleet list.

This is the first *capability probe* in tddy. The telemetry on the same row says how busy a machine
is; this says whether work on it will succeed.

## What a row shows

Two cells, each labelled with the tool it speaks for.

| Cell | State | Reading |
|---|---|---|
| git | configured | the `user.name` and `user.email` commits on that host would carry |
| git | none configured | "Not configured" — the probe ran and the host has no identity set |
| git | could not check | the probe did not run, or its answer was not understood |
| gh | authenticated | the login `gh` there is authenticated as |
| gh | installed, logged out | "Not authenticated" |
| gh | not installed | "Not installed" — `gh` is not on that host's `PATH` |

A host that has not answered yet renders a waiting marker, and a platform that cannot run the probes
at all says so. Neither borrows the shape of a finding.

## Absence is reported, never blanked

Each of the six states sends an operator somewhere different, so none of them collapses into an
empty string or into another. Two distinctions carry the whole design:

- **"Could not check" is not "not configured".** One is a host to go and fix; the other is a probe
  to go and fix. A failed probe rendered as a negative finding sends an operator to set an identity
  that is already set.
- **Unrecognised output is a failure, not a finding.** `gh auth status` has no stable output
  contract, and reporting an authenticated host as logged out is the worse error of the two.

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

## Reading, never writing

The screen reports; it changes nothing. There is no "configure git" or "log in" action on a row, and
no general "run this on host X" primitive exists — the daemon runs two fixed probes and nothing
else.

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

## Technical reference

- Daemon: [`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md),
  [`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md)
- Web: [`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md)
