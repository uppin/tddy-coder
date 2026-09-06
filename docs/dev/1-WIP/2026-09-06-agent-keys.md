# Changeset: agent-keys

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 5/8
**Branch:** `feature/hosts-screen/agent-keys` → base `feature/hosts-screen/host-identity`
**PR:** [#457](https://github.com/uppin/tddy-coder/pull/457)

## Initial Discovery

[`./2026-09-06-agent-keys-initial-discovery.md`](./2026-09-06-agent-keys-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-agent-keys.md`](../../ft/web/1-WIP/PRD-2026-09-06-agent-keys.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | ssh-agent block on the host tooling response |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | ssh-agent client module, socket resolution, probe wiring |
| [`packages/tddy-web`](../../../packages/tddy-web) | ssh-agent section on the Hosts row |

**New external dependencies** (consented, CLAUDE.md § ASK): an ssh-agent protocol client crate, and an
ssh key crate. The key crate is not exercised by this node's behaviour but is added here so node 6
inherits a settled dependency set — its use is node 6's.

## Responsibility

- The ssh-agent client module: connecting to a host user's agent socket, performing
  `REQUEST_IDENTITIES`, and turning `IDENTITIES_ANSWER` into typed identities.
- **Resolving the agent socket for a given OS user** — the central design problem of this node.
- Deriving key type, `SHA256:` fingerprint and comment from each identity blob.
- Reporting four distinct outcomes: keys listed · agent reachable but empty · no agent · probe failed.
- The ssh-agent section on each Hosts row.
- Introducing and pinning the two new crates.

## Boundaries

- Does **not** add, remove or unlock a key. Read-only; node 6 owns every mutation.
- Does **not** handle a passphrase, prompt for one, or carry one on any wire.
- Does **not** touch `packages/tddy-core/src/worktree.rs`'s `GIT_TERMINAL_PROMPT=0` / null-stdin
  hardening, or `DaemonConfig::git.ssh_command`. Node 6 revisits those.
- Does **not** create a second probe RPC — it extends node 4's response message.
- Does **not** report a key's originating file. The agent does not know it.
- Does **not** touch node 4's git or `gh` probes, the telemetry, the registry, the route or the shell.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `host-identity` (#hosts-screen 4/8) | the host tooling probe RPC, its request/response messages, the injectable probe seam and its builder override, and `HostRowTooling` | **adds an ssh-agent block** to that response and a section to that row component | rename, restructure or renumber node 4's messages; change the git or `gh` probes; create a competing probe RPC |
| `host-registry` (#hosts-screen 1/8) | rows with `instance_id` and `online` | addresses the probe per row | change the registry |

> ⚠ **Sequencing.** This node extends a message node 4 owns. Rebase onto the parent
> (`/pr-stack-rebase`) and confirm the tooling probe RPC exists at `HEAD` before `/green`. If the
> ssh-agent block needs a shape node 4's message cannot carry, **stop and raise it** — that is a plan
> change, not a local edit.

## Draft PR contract

Lands first, so node 6 can branch off a real ref:

1. The ssh-agent client module's public surface — the agent connection, the identity type, and the
   socket-resolution seam — which node 6 calls to perform an add.
2. The ssh-agent block on the tooling response, regenerated in both languages.
3. The two new crates in `Cargo.toml` / `Cargo.lock`, so node 6 does not add dependencies and a
   passphrase flow in one PR.
4. The failing tests below.

## Summary

Report whether each host has a reachable ssh-agent and which keys it holds. Nothing in tddy has ever
spoken to an ssh-agent — `Cargo.lock` has no SSH crate at all — so when a session fails to clone or
push for want of a usable key, tddy currently reports a generic git error and nothing about why.

## Scope

- [x] Add and pin the ssh-agent client crate and the ssh key crate
- [ ] Agent socket resolution for a target OS user
- [ ] `REQUEST_IDENTITIES` exchange with a bounded timeout
- [ ] Identity → type / `SHA256:` fingerprint / comment
- [ ] Four distinct outcomes on the wire
- [ ] ssh-agent block on node 4's response + both regenerations
- [ ] Row section rendering the key list
- [ ] Rust unit/integration tests + Cypress component tests
- [ ] **Establish whether a supervised daemon can reach the agent socket at all**

## Technical changes

### State A

- **No SSH crate in the workspace.** `grep -n "name = .*ssh" Cargo.lock` → zero matches.
- **Nothing reads `SSH_AUTH_SOCK`.** Three hits repo-wide: a comment (`dev.daemon.yaml:118`), a
  supervisor policy **test fixture** where it is the example of a *denied* env key
  (`packages/tddy-supervisor/src/policy.rs:487`), and the discovery doc.
- Existing key handling is unrelated and VM-only: `generate_vm_ssh_keypair`
  (`packages/tddy-vm/src/library.rs:377-405`) shells out to `ssh-keygen`; `ssh_opts`
  (`packages/tddy-vm/src/qemu.rs:621-670`) builds argv with `IdentitiesOnly=yes` explicitly "so the
  ambient agent's keys cannot be tried".
- `packages/tddy-remote-git-repo` is **not** an SSH client — it wears git's ssh-argv contract and
  speaks LiveKit, authenticated by daemon tokens (`credentials.rs:18-30`).
- `run_capture_as_user` (`spawner.rs:736-830`) constructs a child environment
  (`PATH = merge_spawn_child_path(None)`); it does not inherit a login session's `SSH_AUTH_SOCK`.

### State B

- The daemon can connect to a host user's ssh-agent and enumerate its identities over the wire
  protocol, bounded by a timeout.
- The tooling probe response carries an ssh-agent block distinguishing all four outcomes.
- Each Hosts row lists the held keys by type, fingerprint and comment.

### Delta

**`Cargo.toml` (workspace) + `Cargo.lock`**
- The ssh-agent client crate and the ssh key crate, pinned.

**`packages/tddy-service`**
- `connection.proto`: an ssh-agent block on node 4's tooling response — an explicit state
  discriminator plus a repeated identity message (type, fingerprint, comment). Field numbers must be
  genuinely free within node 4's message.

**`packages/tddy-daemon`**
- `src/ssh_agent.rs`: the agent connection, the socket-resolution seam, the identity type, the
  fingerprint derivation, the timeout, and the outcome enum.
- `src/host_tooling.rs`: populate the ssh-agent block (node 4 owns the module; this adds one block).
- `src/connection_service.rs`: injectable agent seam for tests, mirroring the existing provider
  overrides.

**`packages/tddy-web`**
- `src/components/hosts/HostRowSshAgent.tsx` — the section and its four states.
- `src/components/hosts/HostRowTooling.tsx` — mount the section (node 4 owns the component).

## Implementation milestones

- [ ] Crates added, workspace builds, `cargo clippy -- -D warnings` clean
- [ ] Socket resolution decided and implemented
- [ ] Identity enumeration against a fake agent
- [ ] Fingerprint derivation matching `ssh-add -l`'s `SHA256:` form
- [ ] Four outcomes on the wire + both regenerations
- [ ] Row section rendering every state
- [ ] Supervised-deployment reachability established and reported

## Testing plan

**Levels.**

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `packages/tddy-daemon/src/ssh_agent.rs` `#[cfg(test)]` | Fingerprint derivation and outcome classification are pure functions of a protocol response |
| Integration (Rust) | `packages/tddy-daemon/src/host_tooling.rs` / `connection_service.rs` `#[cfg(test)]` | The block only reaches the wire through the probe and handler |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | Four states rendering distinguishably is a UI contract |

**Options considered.**

| Option | Verdict |
|---|---|
| A **fake agent listening on a temp-dir Unix socket**, speaking the protocol | **Chosen.** Exercises the real client, is hermetic, and can script an empty list, a populated list, and a hang |
| Talk to the developer's real agent | Rejected — depends on whose machine runs the suite; the testing guide names this anti-pattern directly |
| Mock the client trait only | Insufficient alone — it would never exercise the protocol encoding, which is the part a new crate makes risky. Used **in addition**, at the handler level |

**Fingerprint fixture.** A known public key blob with its published `SHA256:` fingerprint is committed
as a fixture, so the derivation is pinned against a value computed outside this codebase rather than
against our own output.

**The gap this plan cannot close locally.** Whether a supervised daemon can see an agent socket at all
(`resolve_env` is an allowlist, and its own fixture names `SSH_AUTH_SOCK` as denied) cannot be proven
by a unit test. **This is a release blocker for the node's usefulness, not a detail** — establish it
during the red phase and report it in the PR. If a supervised daemon cannot reach the agent, say so
plainly rather than shipping a section that is permanently empty in production.

**Coverage.** Every AC maps to a named test.

## Acceptance tests

**`packages/tddy-daemon/src/ssh_agent.rs`** (unit, against a fake agent socket)

| Test | Validates |
|---|---|
| `lists_the_identities_a_reachable_agent_holds` | AC-1 |
| `reports_an_agent_holding_no_keys_distinctly_from_no_agent` | AC-2 |
| `reports_that_no_agent_is_reachable_when_the_socket_is_absent` | AC-3 |
| `reports_a_probe_failure_when_the_agent_does_not_answer_in_time` | AC-4 |
| `derives_the_sha256_fingerprint_ssh_add_displays` | AC-5 |

**`packages/tddy-daemon/src/connection_service.rs`** (integration)

| Test | Validates |
|---|---|
| `host_tooling_reports_the_ssh_agent_block_for_the_hosts_os_user` | AC-7 |
| `host_tooling_still_reports_git_and_gh_when_the_agent_is_unreachable` | no regression on node 4 |

**`packages/tddy-web/cypress/component/HostsScreenSshAgentAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `lists_each_loaded_key_with_its_type_fingerprint_and_comment` | AC-1, AC-8 |
| `distinguishes_an_empty_agent_from_an_absent_one_from_a_failed_probe` | AC-2, AC-3, AC-4 |
| `presents_a_key_comment_as_a_comment_and_not_as_a_file_path` | AC-6 |

## Decisions & trade-offs

- **The protocol, not `ssh-add`.** `ssh-add -l`'s output is not an API, and the four outcomes would
  have to be inferred from exit codes. The protocol answers all four unambiguously.
- **The crates land in this read-only node.** Node 6 then adds a passphrase flow to a proven
  connection instead of introducing new dependencies and the riskiest flow in the stack together.
- **The key crate is added here but used in node 6.** Slightly unusual, and deliberate: one
  dependency review, in the node where nothing depends on the outcome.
- **No originating file is reported.** The agent does not know it; presenting a comment as a path
  would be a fabricated fact.
- **Extending node 4's message beats a second RPC** — but it makes this node a consumer of a surface
  it does not own, which `## Dependencies` records explicitly.

## Technical debt & production readiness

- ⚠ **Supervised-deployment reachability unresolved at planning time.** If `SSH_AUTH_SOCK` is denied
  by the supervisor's env allowlist, this section is permanently empty in a supervised install and
  node 6 is unreachable there. Establish before this PR is set ready for review.
- ⚠ Two new external dependencies enter the workspace. Both need a pinned version and a note in the PR.

## Refactoring needed

_(populated by each validation phase)_

## Validation results

### Red phase (draft-PR contract)

- Dependencies added and resolved through the host's local crates proxy: **`ssh-agent-lib 0.6.0`**,
  **`ssh-key 0.6.7`**. `ssh-key`'s `encryption` / `rsa` features stay **off** here — enabling them
  belongs to `#hosts-screen 6/8`, where an encrypted private key is actually decrypted.
  (Note: `cargo search` does not work through the proxy — `cargo add` does. A `search` failure is not
  evidence a crate is unavailable.)
- `cargo clippy -p tddy-daemon --all-targets -- -D warnings` — clean.
- `ssh_agent.rs` — **5 tests, 5 red** at the two `unimplemented!()` sites.

### The test harness is a real agent, not a mock

`FakeAgent` binds an actual `UnixListener`, reads the 5-byte header, checks for
`SSH_AGENTC_REQUEST_IDENTITIES` and writes a correctly framed `IDENTITIES_ANSWER`. A mocked client
would exercise none of the encoding, and the encoding is the entire reason this node chose the wire
protocol over `ssh-add`. A `silent()` variant accepts and never answers, so the timeout path is
proven rather than asserted.

The fingerprint fixture is pinned against **OpenSSH's own output** (`ssh-keygen -lf` for a published
ed25519 blob), not against our derivation — otherwise the test would prove only self-consistency.

## TODO

- [x] Record initial discovery (`2026-09-06-agent-keys-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps

## Successor PRs

- [`2026-09-06-agent-add-key.md`](./2026-09-06-agent-add-key.md) — branch
  `feature/hosts-screen/agent-add-key`
