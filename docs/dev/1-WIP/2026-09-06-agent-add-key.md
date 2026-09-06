# Changeset: agent-add-key

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 6/8 — **the largest and riskiest node**
**Branch:** `feature/hosts-screen/agent-add-key` → base `feature/hosts-screen/agent-keys`

## Initial Discovery

[`./2026-09-06-agent-add-key-initial-discovery.md`](./2026-09-06-agent-add-key-initial-discovery.md)

## Related feature documentation

PRD: [`docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`](../../ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md)

Affected: [`projects-screen-multi-host.md`](../../ft/web/projects-screen-multi-host.md)

## Affected packages

| Package | Change |
|---|---|
| [`packages/tddy-service`](../../../packages/tddy-service) | `StreamHostPrompts` + `AnswerHostPrompt` and their messages |
| [`packages/tddy-daemon`](../../../packages/tddy-daemon) | prompt registry + stream handler, host keypair, decryption, agent add |
| [`packages/tddy-web`](../../../packages/tddy-web) | prompt subscription hook, passphrase dialog, `SubtleCrypto` encryption, add-key action |

**New external dependency:** an RSA implementation for the daemon (a third crate, after node 5's two).
**No new web dependency** — `SubtleCrypto` is a platform API.

## Responsibility

- `StreamHostPrompts` (server-streaming) and `AnswerHostPrompt` (unary), with their messages.
- A daemon-side **prompt registry**: issue a prompt, expire it, accept exactly one answer.
- The stream handler, **including the `tokio::select!` on `tx.closed()` teardown**.
- The host's **RSA keypair**, its lifecycle, and publishing its public half with the prompt.
- Browser-side `RSA-OAEP` encryption via `SubtleCrypto`, and the passphrase dialog showing the host and
  the key fingerprint.
- **Key continuity**: pin a host's public key on first sight; block on change with a warning.
- Daemon-side decryption, private-key decryption, the agent add, and dropping the plaintext.
- The add-key action and key selector on the Hosts row.

## Boundaries

- Does **not** persist a passphrase in any form — no vault, no keychain, no "remember me". Explicitly
  rejected in the interview.
- Does **not** log a passphrase, or place it in daemon state beyond the decrypt call.
- Does **not** remove a key from an agent, or generate one.
- Does **not** change `packages/tddy-core/src/worktree.rs`'s git hardening **unless the red phase proves
  it necessary** — the in-process decrypt path is expected to make that unnecessary.
- Does **not** change node 5's agent client, its socket resolution, or the key listing — it calls them.
- Does **not** change node 4's git/`gh` probes, the telemetry, the registry, the route or the shell.
- Does **not** introduce a general server-asks-the-UI primitive for the whole app. This node solves the
  passphrase case; generalizing needs a second caller.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `agent-keys` (#hosts-screen 5/8) | `src/ssh_agent.rs` — the agent connection, the socket-resolution seam, the identity type; **and the ssh key crate already in the workspace**; plus `HostRowSshAgent` and the ssh-agent block on the tooling response | calls the agent connection to perform the add, uses the key crate to decrypt the private key, and adds an action to the existing row section | change the agent client, socket resolution, fingerprint derivation, the key listing, or the ssh-agent block's shape |
| `host-identity` (#hosts-screen 4/8) | the tooling probe RPC and `HostRowTooling` | reads the agent state to decide whether an add is offered | change the probe RPC or the git/`gh` probes |
| `host-registry` (#hosts-screen 1/8) | rows with `instance_id` and `online` | addresses the prompt flow at a specific host | change the registry |

> ⚠ **Sequencing.** This node cannot start until node 5's agent client exists at `HEAD` — it is the
> thing that performs the add. Rebase onto the parent (`/pr-stack-rebase`) and run node 5's dependency
> gate before `/green`.

## Draft PR contract

Lands first, so node 7 can branch off a real ref:

1. `StreamHostPrompts` + `AnswerHostPrompt` and their messages, regenerated in both languages.
2. The prompt registry's public surface and the host-keypair seam.
3. The failing tests below — **including the teardown regression test**, which is the one most easily
   forgotten and the one whose absence leaks a task per subscription.

## Summary

Let an operator load a key into a host's ssh-agent from the browser: the host asks for the passphrase,
tddy-web prompts, and the answer travels back encrypted under that host's public key. Nothing in tddy
prompts for a secret from the server today, and the daemon is deliberately hardened against interactive
prompts — so this node builds the channel, the crypto and the mutation together.

## Scope

- [ ] Prompt registry: issue, expire, single-use answer
- [ ] `StreamHostPrompts` handler **with `tx.closed()` teardown**
- [ ] `AnswerHostPrompt` handler, auth + unknown-id rejection
- [ ] Host RSA keypair and its lifecycle
- [ ] Public key published with the prompt; fingerprint derivation
- [ ] Browser `SubtleCrypto` `RSA-OAEP` encryption
- [ ] Key continuity: pin on first sight, block on change
- [ ] Daemon decrypt → private key decrypt → agent add → drop plaintext
- [ ] Passphrase dialog + add-key action + key selector
- [ ] Rust unit/integration tests, teardown test, Cypress round-trip tests
- [ ] Confirm whether the git hardening needs any change at all

## Technical changes

### State A

- **No server-initiated prompt on `ConnectionService`.** The only server-asks-UI mechanism is the ACP
  bidi stream (`packages/tddy-service/proto/tddy/acp/v1/acp.proto:18-55`, hook
  `packages/tddy-web/src/components/chat/useAcpSession.ts:129`, question `:381`, reply `:479`).
  Weaker, notification-only signals exist: `SessionEntry.pending_elicitation`,
  `StreamSessionNotifications` (`ATTENTION_REQUIRED`), the Codex OAuth metadata relay.
- **Existing passphrase dialogs run the other way** — `ScreenSharingPassphraseDialog.tsx`,
  `VncPassphraseDialog.tsx`: the UI asks before a unary call.
- **The daemon is hardened against prompts**: `worktree.rs:39-52` closes stdin and sets
  `GIT_TERMINAL_PROMPT=0`; `config.rs:192-200` documents the `BatchMode=yes` workaround.
- **An encrypted-secret precedent exists but is the wrong model** —
  `packages/tddy-daemon/src/screen_sharing_vault.rs` *stores* credentials; we must not.
- **`mint_local_token`** (`connection_service.rs:~16481`) is the precedent for a transport-restricted
  method.
- After node 5: an agent client and an ssh key crate exist; no RSA crate does.

### State B

- The daemon can pose a question to the UI and await an answer, over a stream + unary pair.
- The answer is encrypted end to end under the host's published RSA public key.
- The host decrypts, unlocks the private key, adds it to the agent, and drops the plaintext.
- The browser pins each host's key and refuses to proceed silently if it changes.

### Delta

**`Cargo.toml` (workspace) + `Cargo.lock`** — an RSA implementation, pinned.

**`packages/tddy-service`**
- `connection.proto`: `rpc StreamHostPrompts(...) returns (stream HostPromptEvent);` and
  `rpc AnswerHostPrompt(...) returns (AnswerHostPromptResponse);` plus a prompt message carrying the
  prompt id, host, subject key, the host public key and its fingerprint; and an answer message
  carrying the prompt id and the **encrypted** payload. Field numbers must be genuinely free.

**`packages/tddy-daemon`**
- `src/host_prompts.rs`: the registry — issue / expire / answer-once — and the prompt types.
- `src/host_keypair.rs`: keypair generation, persistence of the private half with owner-only
  permissions, public-half publication, fingerprint derivation.
- `src/connection_service.rs`: both handlers, the stream's `tokio::select!` teardown, auth, injectable
  seams for tests.
- `src/connection_tonic_adapter.rs`: the stream + unary adapter entries.
- `src/ssh_agent.rs`: **called, not changed** — node 5 owns it.

**`packages/tddy-web`**
- `src/rpc/useHostPrompts.ts` — the subscription hook, following the `useHostStats` template
  (`useEffect` + `cancelled` + `for await` + swallow AbortError).
- `src/lib/hostKeyPinning.ts` — pin on first sight, detect change.
- `src/lib/encryptForHost.ts` — `SubtleCrypto` `RSA-OAEP` encryption.
- `src/components/hosts/HostPassphraseDialog.tsx` — the dialog, host name + fingerprint.
- `src/components/hosts/HostRowSshAgent.tsx` — the add-key action (node 5 owns the section).

## Implementation milestones

- [ ] Proto + both regenerations
- [ ] Prompt registry with expiry and single-use, unit-tested
- [ ] Stream handler **and its teardown test green** — before any UI work
- [ ] Host keypair + fingerprint
- [ ] Browser encryption producing a payload the daemon decrypts
- [ ] Full round trip adding a real key to a fake agent
- [ ] Key pinning + change warning
- [ ] Wrong-passphrase, expiry and replay paths
- [ ] Confirm the git hardening question

## Testing plan

**Levels.** Four, and the teardown one is not optional.

| Level | Where | Why |
|---|---|---|
| Unit (Rust) | `src/host_prompts.rs`, `src/host_keypair.rs` `#[cfg(test)]` | Expiry, single-use and fingerprint derivation are pure logic |
| Integration (Rust) | `src/connection_service.rs` `#[cfg(test)]` | Auth, unknown-prompt rejection, and decrypt→add only exist at the handler |
| **Teardown (Rust, wire-level)** | `packages/tddy-daemon/tests/` | A leaked task is invisible to every other level. Mirrors `stream_livekit_rooms_rpc.rs::stops_reading_the_server_once_the_subscriber_is_gone` |
| Component (Cypress) | `packages/tddy-web/cypress/component/` | The prompt→answer round trip and, critically, that the payload is not plaintext |

**Options considered.**

| Option | Verdict |
|---|---|
| Fake agent socket (node 5's harness) + injected keypair + scripted prompts | **Chosen.** Hermetic, exercises real crypto both ways |
| Assert crypto by round-tripping through our own code only | Insufficient alone — a fixture encrypted by an independent implementation pins the format |
| Skip the teardown test because "the stream is short-lived" | **Rejected.** It is silent by nature; the codegen doc names this exact case |

**The assertion that must not be forgotten.** A round-trip test that only checks the key was added
would pass equally well with the passphrase in the clear. **`AnswerHostPrompt`'s recorded payload must
be asserted to not contain the passphrase.** Unary calls *are* recorded by the in-memory backend
interceptor (`packages/tddy-connectrpc-testkit/src/backend.ts:147-157`), so `callsTo(...)` makes this
directly assertable.

**Log hygiene.** A test must assert the passphrase does not appear in captured daemon logs — AC-5 is
otherwise unverified, and a stray `debug!` is exactly how such a secret escapes.

**Coverage.** Every AC maps to a named test.

## Acceptance tests

**`packages/tddy-daemon/src/host_prompts.rs`** (unit)

| Test | Validates |
|---|---|
| `an_unanswered_prompt_expires_and_releases_its_operation` | AC-6 |
| `a_prompt_accepts_exactly_one_answer` | AC-7 |
| `an_unknown_prompt_id_is_rejected` | AC-11 |

**`packages/tddy-daemon/src/host_keypair.rs`** (unit)

| Test | Validates |
|---|---|
| `derives_a_stable_fingerprint_for_a_published_public_key` | AC-8 |
| `decrypts_a_payload_encrypted_against_its_published_public_key` | AC-2 |

**`packages/tddy-daemon/src/connection_service.rs`** (integration)

| Test | Validates |
|---|---|
| `answer_host_prompt_rejects_an_invalid_token` | AC-11 |
| `a_correct_passphrase_adds_the_key_to_the_agent` | AC-3 |
| `an_incorrect_passphrase_reports_failure_and_adds_nothing` | AC-4 |
| `the_passphrase_never_appears_in_captured_logs` | AC-5 |

**`packages/tddy-daemon/tests/stream_host_prompts_rpc.rs`** (wire-level)

| Test | Validates |
|---|---|
| `stops_the_prompt_pump_once_the_subscriber_is_gone` | AC-10 — the leak |

**`packages/tddy-web/cypress/component/HostAddKeyAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `surfaces_a_passphrase_prompt_naming_the_host_and_the_key` | AC-1 |
| `sends_an_encrypted_answer_that_does_not_contain_the_passphrase` | AC-2 — the load-bearing one |
| `shows_the_hosts_public_key_fingerprint_in_the_dialog` | AC-8 |
| `blocks_the_flow_with_a_warning_when_a_hosts_key_has_changed` | AC-9 |
| `reports_a_failure_without_adding_a_key_when_the_passphrase_is_wrong` | AC-4 |

## Decisions & trade-offs

- **Server-stream + unary reply, not ACP bidi.** Simpler, mirrors an in-repo pair, and the reply is a
  unary call that can be transport-restricted like `mint_local_token`. Cost: two methods instead of one,
  and correlation by prompt id rather than envelope id.
- **RSA-OAEP, no hybrid envelope.** A passphrase fits in a 2048-bit OAEP payload; a hybrid AEAD scheme
  would be more code for no benefit at this size.
- **`SubtleCrypto`, so no new web dependency.**
- **Key continuity (TOFU) + visible fingerprint.** ⚠ **The choice most worth challenging in review.**
  Encryption alone defeats a passive relay but not an active peer publishing its own key, because the
  common room is explicitly not cryptographically authenticated. Pinning bounds that; the weaker
  alternative is to accept passive-only protection and disclose it. Recorded to be argued with.
- **Never persist the passphrase.** No vault, despite `screen_sharing_vault.rs` being right there.
- **The git hardening is expected to be untouched**, because the decrypt happens in-process with no
  subprocess and no TTY — a direct dividend of node 5 choosing the agent protocol over `ssh-add`.

## Technical debt & production readiness

- ⚠ **Blocked on node 5's finding.** If a supervised daemon cannot reach the agent socket
  (`resolve_env` is an allowlist naming `SSH_AUTH_SOCK` as denied in its own fixture), this node is
  unreachable in a supervised install. Confirm before starting.
- ⚠ **A third external crate.** Pin it and justify it in the PR.
- ⚠ **Host private key at rest** — owner-only permissions, and a documented rotation story.
- ⚠ **If the red phase shows this node is too large to review**, split **by capability**: an IPC-only
  add first, then the encrypted remote path. Never by layer.

## Refactoring needed

_(populated by each validation phase)_

## Validation results

_(populated by each validation command)_

## TODO

- [x] Record initial discovery (`2026-09-06-agent-add-key-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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

- [`2026-09-06-desktop-probe.md`](./2026-09-06-desktop-probe.md) — branch
  `feature/hosts-screen/desktop-probe`
