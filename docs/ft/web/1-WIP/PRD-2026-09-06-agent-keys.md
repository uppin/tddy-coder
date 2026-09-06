# PRD — Hosts screen: each host's ssh-agent and the keys it holds

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (screen) + `daemon` (agent client)
**Stack:** `#hosts-screen` node 5 of 8
**Branch:** `feature/hosts-screen/agent-keys` → base `feature/hosts-screen/host-identity`

## Affected features

| Document | Relationship |
|---|---|
| [`PRD-2026-09-06-host-identity.md`](./PRD-2026-09-06-host-identity.md) | Owns the host tooling probe RPC; this node adds an ssh-agent block to it |
| [`PRD-2026-09-06-host-registry.md`](./PRD-2026-09-06-host-registry.md) | Supplies the rows |

## Summary

Report, per host, whether an **ssh-agent is reachable** and, when it is, **which keys it currently
holds** — key type, fingerprint and comment.

This is read-only. Loading a key is node 6.

## Background

Nothing in tddy has ever spoken to an ssh-agent. `Cargo.lock` contains **no SSH crate of any kind**,
and nothing reads `SSH_AUTH_SOCK`. The only occurrences of the term in the repo are a config comment
and — tellingly — a **supervisor policy test fixture** using `SSH_AUTH_SOCK` as the example of an
environment key that is **denied** to a brokered child.

The operator consequence today: when a session cannot clone or push because the host's agent holds no
usable key, tddy reports a generic git failure and nothing about why.

## Proposed changes

### What is changing

- **A new dependency: an ssh-agent protocol client crate**, with explicit developer consent already
  given. The daemon speaks `REQUEST_IDENTITIES` / `IDENTITIES_ANSWER` rather than parsing `ssh-add -l`
  output.
- **An ssh-agent block added to node 4's host tooling probe response**, reporting agent reachability
  and the identity list.
- **An ssh-agent section on each Hosts row** listing the held keys.

### Why the protocol, not `ssh-add`

`ssh-add -l`'s output is human-readable text, not an API, and "no agent" versus "agent with no keys"
versus "`ssh-add` not installed" have to be inferred from exit codes. The wire protocol returns a typed
identity list and distinguishes all of those unambiguously. Introducing the crate in this read-only
node means node 6 inherits a proven connection instead of adding the crate and the passphrase flow at
once.

### What a key row can honestly say

From `IDENTITIES_ANSWER` the daemon derives, per identity: **key type**, a **SHA256 fingerprint** (what
`ssh-add -l` displays), and the **comment**.

⚠ **A key's originating file is not reported, because the agent does not know it.** The comment is
arbitrary text set at key-generation time and must not be presented as a path.

### What is staying the same

- **Nothing is changed on any host.** No key is added, removed or unlocked in this node.
- The `GIT_TERMINAL_PROMPT=0` / null-stdin hardening in `packages/tddy-core/src/worktree.rs:39-52`
  is untouched — this node reads agent state and never prompts.
- Node 4's git and `gh` probes, the telemetry, the registry, the route and the screen shell are
  unchanged.

## Impact analysis

### Technical

- ⚠ **Locating the agent socket for the target user is the central design problem**, and this PRD must
  settle it rather than leaving it to the green phase. `SSH_AUTH_SOCK` is a per-user, per-session
  environment variable; `spawner::run_capture_as_user` **constructs** a child environment
  (`PATH = merge_spawn_child_path(None)`) rather than inheriting a login session's, so it does not
  carry one automatically.
- ⚠ **The supervisor may deny it outright.** `resolve_env`
  (`packages/tddy-supervisor/src/policy.rs:124`) is an allowlist, and its own test fixture names
  `SSH_AUTH_SOCK` as a denied key. In a supervised deployment the daemon may not see the agent socket
  at all. **This must be established in the red phase** — it decides whether this node is useful in
  production, and whether node 6 is reachable at all.
- Two new external crates enter the workspace (an agent client, and a key crate node 6 needs). Both
  were consented to; both must be pinned and justified in the PR.
- The probe must be **bounded**: an unresponsive agent socket must time out into a reported failure,
  never hold the RPC open.

### User

- Each host row shows whether its agent is reachable and which keys it holds. A host whose agent is
  empty is visibly different from one with no agent at all.

## Acceptance criteria

- [ ] **AC-1** A host with a reachable agent holding keys lists each key's type, fingerprint and comment.
- [ ] **AC-2** A host with a reachable agent holding **no** keys reports "no keys loaded" — distinct
      from having no agent.
- [ ] **AC-3** A host with **no reachable agent** reports that, distinctly.
- [ ] **AC-4** An agent that does not answer within the timeout reports a probe failure, distinct from
      every state above.
- [ ] **AC-5** Fingerprints are reported in the `SHA256:` form `ssh-add -l` displays.
- [ ] **AC-6** A key's comment is presented as a comment, never as a file path.
- [ ] **AC-7** The agent is resolved for the **host's OS user**, matching node 4's probe scoping.
- [ ] **AC-8** The row renders the key list without truncating a fingerprint into ambiguity.

## Out of scope for this node

Adding, removing or unlocking a key (node 6). Any passphrase handling. Any change to the git remote
hardening. VNC/RDP (nodes 7–8).

## Successor PRs

- `feature/hosts-screen/agent-add-key` — load a key into a host's ssh-agent from the browser.
