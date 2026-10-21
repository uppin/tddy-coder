# Host tooling on the Hosts screen

Every row of the Hosts screen reports what that host has installed and configured: the **git
identity** its commits would carry, the state of the **GitHub CLI** on it, the **ssh-agent** it has
with the keys that agent is holding, the **SSH connections** its OpenSSH config names, and whether a
**remote desktop** on it can be reached — and bridged.

## Motivation

Nowhere in tddy says what a host will commit as. A session runs on a machine, makes commits there
and pushes them from there, and all three depend on facts that live in that machine's own home
directory — `~/.gitconfig`, `~/.config/gh/hosts.yml`, and the agent that user's login session
started. When a host commits under the wrong identity, or its `gh` is logged out so a push fails, or
its agent holds no key its remotes accept, the operator finds out from the result rather than from
the fleet list. A clone or push that fails for want of a usable key reports a generic git error and
nothing about why.

The remote desktop is the same story told by a different failure. tddy bridges VNC and RDP per
session, but the session flow asks nothing about availability first: a host with no desktop serving,
or a daemon with no bridge binary to spawn, is discovered only when the stream fails to start — after
the operator has already asked for one.

This is the first *capability probe* in tddy. The telemetry on the same row says how busy a machine
is; this says whether work on it will succeed.

## What a row shows

Five sections, each labelled with the tool it speaks for.

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
| ssh connections | aliases listed | one span per explicit `Host` alias from that host's `~/.ssh/config` |
| ssh connections | none | "No SSH hosts" — the config was read and names no explicit aliases |
| ssh connections | could not check | the listing failed; must not read as "no destinations" |
| remote desktop | bridge present | "Bridge ready" — this host's daemon has the bridge binary for that protocol |
| remote desktop | no bridge | "No bridge" — nothing to spawn, whatever is or is not serving |
| remote desktop | desktop serving | "Desktop on :5900" — something accepted a connection on the port named |
| remote desktop | nothing serving | "No desktop on :5900" — the probe reached the host and nothing was listening |
| remote desktop | could not check | "Could not check :5900", with the reason on hover |
| remote desktop (X11 display) | display available | "Display :1" with "Display available" — the daemon's user can connect to that display's X server socket |
| remote desktop (X11 display) | no x11vnc | "Display :1" with "No bridge" — listed, with no Connect, until `x11vnc` is installed |

The remote-desktop section lists the host's **desktop channels**. Two are always there: VNC on
`:5900` and RDP on `:3389`, including the ones nothing answered for. "No desktop on :5900" is a
finding, and a row that listed only the protocols that answered could not be told apart from one
where nobody looked. Beside them, the section has **one channel per X11 display** the daemon's own OS
user can open, named "Display :N". These are the `X<N>` sockets in the X11 socket directory
(`/tmp/.X11-unix` unless configured) that accept a connection from that user. A stale socket left
by a crashed server is not listed, and neither is another user's display. A display already served
on its VNC port (`5900 + N`) is not listed a second time. Each connectable channel has its own
Connect. Pressing Connect on an X11 channel has the daemon start `x11vnc` for that display, bound
to loopback with no password, and the stream runs through it (see
[screen-sharing-sessions.md](./screen-sharing-sessions.md) § Connecting from a host row). Without
`x11vnc` on the host, its displays are still listed and report "No bridge", so the operator learns
what to install.

A host that has not answered yet renders a waiting marker, and a platform that cannot run the probes
at all says so. Neither borrows the shape of a finding. The desktop section is the exception to the
second half: it is a TCP connect to the host's own loopback plus a check that a file exists, so it
answers on every platform and is never reported as unsupported.

## Absence is reported, never blanked

Every one of these states sends an operator somewhere different, so none of them collapses into an
empty string or into another. Four distinctions carry the whole design:

- **"Could not check" is not "not configured".** One is a host to go and fix; the other is a probe
  to go and fix. A failed probe rendered as a negative finding sends an operator to set an identity
  that is already set.
- **Unrecognised output is a failure, not a finding.** `gh auth status` has no stable output
  contract, and reporting an authenticated host as logged out is the worse error of the two.
- **"No keys loaded" is never said about an agent that did not answer.** Only an agent that
  answered can report an empty list; every other emptiness is "No agent" or "Could not check". An
  empty agent wants a key added, an absent one wants an agent started, and a failed probe wants
  looking at on the daemon side.
- **"No SSH hosts" is never said when the config could not be read.** Key listing collapses an
  unreadable `~/.ssh` into an empty offer; SSH destination listing does not, because empty here means
  LocalShell is the only choice in later session flows.
- **"No desktop" is never said about a port nobody reached.** A refused connection is an answer —
  we got to the host and nothing was listening. A timeout, an unreachable network or a denied
  connect is not: it reads "Could not check", with the port and the reason. A row that keyed off
  reachability alone would report "No desktop" for a host it never touched.

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

## Two availabilities, and the row keeps them apart

"This host has a desktop available" is two claims, and only one of them is about the desktop:

| Fact | What it means | What a "no" asks for |
|---|---|---|
| **Bridge capability** | this host's daemon can spawn a VNC/RDP bridge at all | install the bridge binary on that host |
| **Desktop reachability** | something is serving a desktop on the checked port | start a desktop server, or look at the port |

They are independent — a host can serve a desktop that tddy has no bridge for, and a host with both
bridges installed can be serving nothing — so the row shows both, side by side, and never one
merged verdict. Merged, "unavailable" sends an operator to the wrong machine's worth of work.

**The port that was checked is always named.** The endpoint probes read only the default ports, and
a host is free to serve elsewhere; "No desktop" without a port reads as authoritative about the host
rather than about `:5900`. A host's X11 displays are found by their sockets, not by port, so a
display with no VNC server of its own is still offered. Discovering other non-default ports is not
part of this.

**The probe is a bare TCP connect, closed immediately — never a protocol handshake.** The screen
would otherwise be pointing half-open RFB handshakes at people's desktops on a timer, and the
connect already answers the question being asked.

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
[`packages/tddy-host-service/docs/host-tooling-probe.md`](../../../packages/tddy-host-service/docs/host-tooling-probe.md#reaching-the-socket-on-a-supervised-host).

## Reporting, and the one action

The probes themselves change nothing. There is no "configure git" and no "log in" action, and no
general "run this on host X" primitive exists — the daemon runs a fixed set of probes and nothing else.

The ssh-agent section carries one of the screen's two write actions: **loading a key into that host's
agent**, offered on a row whose agent answered, and only there. A host whose agent did not answer
needs an agent started, not a key loaded. The passphrase for that key is asked for by the host,
answered in the browser, and carried back encrypted under that host's own public key; it is never
persisted, logged or written to disk. That flow is its own feature —
[hosts-screen-add-key.md](./hosts-screen-add-key.md) — and nothing else in that section writes anything.

Removing a key from an agent, and generating one, are not offered.

The desktop section is the other. Its **probe** starts no stream, spawns no bridge and creates no
target — it reports what is reachable and nothing more. **Opening** a host's desktop is a separate
action on the same row, and that one does write: it attaches a target to the host and starts a bridge
process. It is described in
[`screen-sharing-sessions.md`](./screen-sharing-sessions.md) rather than here.

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
- [x] A host serving VNC on the checked port reports VNC reachable.
- [x] A host serving RDP on the checked port reports RDP reachable.
- [x] A host serving neither reports both unreachable, naming the ports checked.
- [x] A host whose bridge binary is missing reports that it cannot bridge — distinctly from a
      desktop being unreachable.
- [x] A probe that could not reach a port reads as a probe failure, distinct from "no desktop".
- [x] The probe never performs a protocol handshake — a bare TCP connect, closed immediately.
- [x] The row shows bridge capability and desktop reachability separately and never conflates them.
- [x] A host whose `~/.ssh/config` defines explicit `Host` aliases lists each alias on the row.
- [x] A readable config with no explicit aliases reads "No SSH hosts", not a probe failure.
- [x] An unreadable config reads as "Could not check", never as "No SSH hosts".
- [x] Wildcard-only `Host` patterns contribute no alias names.
- [x] In the running app, a reachable desktop on an online host shows Connect on its Hosts row.
- [x] An offline host makes no tooling call, even when a wire to it exists.
- [x] A host with displays `:0` and `:1` lists two X11 channels plus the endpoint readings.
- [x] A display socket nothing answers on, and an entry not named `X<N>`, is not listed.
- [x] A display already served on its VNC port is not listed twice.
- [x] Without `x11vnc` the X11 channels are listed but offer no Connect.
- [x] A row with two connectable channels offers two Connect actions, each opening its own channel.
- [x] A host that does not answer reads "Tooling could not be read", never a blank cell.
- [x] Two concurrent requests for one host run one probe.
- [x] A request within the TTL runs no probe, and one after it probes again.
- [x] Different hosts, different OS users on one host, and different GitHub hostnames are cached separately.

## Where the cells appear

Every **online** row on the Hosts screen carries a tooling cell. The cell asks that row's host for
its tooling — one `GetHostTooling` per online row per visit, addressed to the host the row names —
and renders the answer as the section described above, remote desktop and its Connect action
included. While the answer is pending the cell shows a loading marker. When the host does not answer
the cell says **"Tooling could not be read"**, never a blank: a blank cell reads as a host with no
tooling, which is a finding nobody made.

An **offline** row shows no tooling and asks nothing — a host that has left the roster has nothing
to answer through.

The daemon bounds the cost of this. It keeps each host's answer for 30 seconds, per OS user and per
GitHub hostname, and callers that arrive while a probe for the same host is running share that one
run. A Hosts screen listing many hosts therefore does not start a subprocess fan-out on every visit.
A failed reading is part of the answer and is kept for the same window, so an operator who fixes a
host's `gh` login sees it within that window.

The remote-desktop section is mounted by `HostRowTooling` behind an optional prop, the way the
ssh-agent section is, so it appears with the section and needs no wiring of its own. It fetches
nothing itself.

## Technical reference

- Daemon: [`packages/tddy-host-service/docs/host-tooling-probe.md`](../../../packages/tddy-host-service/docs/host-tooling-probe.md),
  [`host-service.md`](../../../packages/tddy-host-service/docs/host-service.md)
- Web: [`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md)
- Feature: [hosts-screen-add-key.md](./hosts-screen-add-key.md) — loading a key into the agent this
  section reports on
- Related: [`screen-sharing-sessions.md`](./screen-sharing-sessions.md) — the per-session VNC/RDP
  bridging whose binaries the desktop section checks for
