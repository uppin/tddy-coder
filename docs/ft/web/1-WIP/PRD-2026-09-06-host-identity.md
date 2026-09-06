# PRD — Hosts screen: each host's git identity and GitHub CLI status

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (screen) + `daemon` (probe)
**Stack:** `#hosts-screen` node 4 of 8
**Branch:** `feature/hosts-screen/host-identity` → base `feature/hosts-screen/host-resources`

## Affected features

| Document | Relationship |
|---|---|
| [`PRD-2026-09-06-host-registry.md`](./PRD-2026-09-06-host-registry.md) | Supplies the rows this node adds a tooling section to |
| [`docs/ft/web/projects-screen-multi-host.md`](../projects-screen-multi-host.md) | Defines peer RPC routing and the common-room trust model |

## Summary

Report, per host, the **git identity that host is configured with** (`user.name`, `user.email`) and the
status of the **GitHub CLI** on it: whether `gh` is installed, whether it is authenticated, and as
which login.

This is the first *host tooling probe* in tddy. Nodes 1–3 report facts the daemon already holds; this
one asks what is installed and configured on the machine, and it establishes the RPC shape that the
ssh-agent (node 5) and remote-desktop (node 7) probes extend.

## Background

Neither fact is available anywhere today. No production code reads `git config user.name` or
`user.email` — every hit in the workspace is a test fixture *writing* an identity into a temp repo, and
`packages/tddy-daemon/src/session_room.rs:338` records that commits are deliberately "Signed by the
daemon under a fixed identity rather than by whatever `user.email` the checkout" has. And nothing
shells out to `gh` at all: the only `gh` in the repo is `scripts/ci-status.sh`, developer tooling.

The operator consequence is that when a host's commits are attributed to the wrong person, or a push
fails because `gh` is logged out there, nothing in tddy says so.

## Proposed changes

### What is changing

- **One new unary RPC** on `ConnectionService` reporting a host's tooling status, carrying a
  `daemon_instance_id` so it is relayed to that host by the existing peer routing.
- **A daemon-side probe module** running the two lookups through
  `spawner::run_capture_as_user` — as the host's OS user, because both facts are user-scoped
  (`git config` reads `$HOME/.gitconfig`; `gh auth status` reads `$HOME/.config/gh/hosts.yml`).
- **A tooling section on each Hosts row** showing the git identity and the `gh` state.

### Absence is reported, not blanked

Six distinct outcomes must be distinguishable on the wire, because collapsing any of them into an
empty string would be a fabricated value an operator acts on:

| Fact | States |
|---|---|
| git identity | configured (name + email) · not configured · probe failed |
| `gh` | not installed · installed but logged out · authenticated as `<login>` |

### What is staying the same

- **No general remote-execution RPC is introduced.** This node ships two fixed probes, not a
  "run this on host X" primitive — that is a far larger security surface.
- `ExecuteTool` is untouched; it requires a `session_id` and a host probe has no session.
- `packages/tddy-github` (OAuth web login) is untouched. It is unrelated to `gh`.
- Telemetry, the registry, the route and the screen shell are earlier nodes' and are not modified.

## Impact analysis

### Technical

- **Cross-host routing is free.** `rpc_served_by_peer`
  (`packages/tddy-daemon/src/connection_service.rs:9171-9183`) already relays any unary
  `ConnectionService` RPC to `daemon-{instance_id}`. No transport work.
- ✅ **The risk was real; the mechanism named here was not.** This section originally pointed at the
  supervisor's `resolve_env` / `resolve_tool_path` allowlists
  (`packages/tddy-supervisor/src/policy.rs`). Neither is on this path — `spawner::run_output_as_user`
  forks and drops privilege itself, with no supervisor brokering.

  The actual gap was the **daemon's own** `resolve_tool_path`, which anchors a relative program name
  to the daemon's process cwd and never searches `PATH`: bare `git` / `gh` resolved to
  `<daemon-cwd>/git`, which under systemd is `/git`. The probe could not have run anywhere, and `gh`
  would have reported "not installed" on every host on earth. Resolved in green by looking both
  programs up on the child's `PATH` before spawning. The lesson worth keeping: this was provable by
  inspection from the start, and was missed only because no test exec'd anything.

- ⚠ **An unprivileged daemon cannot probe another OS user.** The privilege drop calls `setgid` /
  `setuid` directly. Under `./install --systemd` the daemon is an unprivileged child of
  `tddy-supervisor`, so probing any OS user but its own returns `EPERM` and reports a probe failure.
  Pre-existing for `run_capture_as_user`; this feature is the first to exercise it per-host. **AC-7 is
  not satisfiable on that deployment shape** until it is addressed.
- **Probes must be bounded.** A hung `gh` (network stall on `auth status`) must time out and report a
  probe failure rather than holding the RPC open.
- ⚠ **Three unrelated GitHub identities can disagree** — the tddy web session's login, a
  `GITHUB_TOKEN`/`GH_TOKEN` in the environment, and the host's `gh` login. This node reports only the
  last, and the UI must label it as such so it is not read as the tddy user in `UserAvatar`.

### User

- Each host row gains a tooling section. An operator can see at a glance that a host will commit under
  the wrong identity, or that its `gh` is logged out.

## Acceptance criteria

- [ ] **AC-1** A host with a configured git identity reports its `user.name` and `user.email`.
- [ ] **AC-2** A host with **no** git identity reports "not configured" — not an empty name.
- [ ] **AC-3** A host without `gh` reports "not installed".
- [ ] **AC-4** A host with `gh` installed but logged out reports "not authenticated".
- [ ] **AC-5** A host with `gh` authenticated reports the login it is authenticated as.
- [ ] **AC-6** A probe that fails or times out reports a probe failure, distinct from every state above.
- [ ] **AC-7** The probes run as the **host's OS user**, so a per-user `~/.gitconfig` is what is read.
- [ ] **AC-8** The RPC rejects an invalid `session_token`.
- [ ] **AC-9** A request naming another host is served by that host, via the existing peer routing.
- [ ] **AC-10** The row labels the `gh` login as the host's, distinguishably from the tddy session user.

## Out of scope for this node

ssh-agent (nodes 5–6), VNC/RDP (nodes 7–8), any general remote-exec RPC, any change to tddy's own
GitHub OAuth login, and any *action* on a host (this node reads; it changes nothing).

## Successor PRs

- `feature/hosts-screen/agent-keys` — each host's ssh-agent and the keys it holds.
