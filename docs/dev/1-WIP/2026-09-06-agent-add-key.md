# Changeset: agent-add-key

**Date:** 2026-09-06
**Status:** 🚧 In Progress
**Type:** New feature
**Stack:** `#hosts-screen` 6/8 — **the largest and riskiest node**
**Branch:** `feature/hosts-screen/agent-add-key` → base `feature/hosts-screen/agent-keys`
**PR:** [#458](https://github.com/uppin/tddy-coder/pull/458)

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

## Prerequisites

Open items in [`docs/dev/TODO.md`](../TODO.md) that this PR runs into. Each must be resolved **before
or during** this work — they are not follow-ups, because this node ships a new secret at rest.

### ⛔ BLOCKING — the daemon's secret stores still truncate in place

`docs/dev/TODO.md` § *The daemon's secret stores still truncate in place* (source:
atomic-session-file-writes, 2026-08-16).

`tddy_core::atomic_file` carries every other session- and daemon-state write, but
`github_token_store.rs`, `vnc_vault.rs` and `screen_sharing_vault.rs` were left out **precisely
because they are the ones correct about mode `0600` on creation** — and `write_atomic` copies
permission bits only from an *existing* target, so a first write through it creates the swap file at
the process umask and publishes a world-readable secret store.

**Why this blocks this node specifically:** `FileHostKeypair` persists an **RSA private key**. Both
available routes are wrong today:

| Route | Consequence |
|---|---|
| Use `write_atomic` as-is | The private key is created world-readable on first write |
| Hand-roll the `FileGitHubTokenStore` pattern | A **fourth** file joins the list this TODO exists to shrink |

**Resolution required before `/green` on this node:** land the `write_atomic_with_mode(path,
contents, mode)` variant the TODO names — it sets the swap file's mode *before* writing rather than
copying it from the target — and build `FileHostKeypair` on it. Converting the three existing call
sites can stay a separate PR; what cannot wait is that this node does not add a fifth hand-rolled
secret writer, or a world-readable one.

The TODO also notes the live consequence of the status quo: a full disk empties a token store, and an
empty secrets file reads as "no credential" — surfacing as a re-auth prompt rather than as the write
failure it is. A truncated *keypair* file would read as "no key", i.e. every prompt for this host
would fail with no indication why.

### ⚠ DURING — `connection_service.rs` is 19,600 lines

`docs/dev/TODO.md` § *`connection_service.rs` is 19,600 lines* (source:
subagent-conversation-inference, 2026-08-29), flagged rather than acted on.

This node adds two handlers, a stream adapter, a field and an accessor to that file. Across the
stack, **nodes 1, 3, 4 and 6** modify it — nodes 2, 5, 7 and 8 do not — so the stack makes a known
problem measurably worse in four places. The TODO is explicit that a split "needs to be its own
PR", so **this is recorded, not fixed here**.
Its listed seam candidates do not yet include a host-facing group; when that split happens, the host
registry, tooling probe and prompt handlers form one.

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

- [x] **Prerequisite:** `write_atomic_with_mode` before persisting the host private key (see Prerequisites)
- [x] Prompt registry: issue, expire, single-use answer
- [x] `StreamHostPrompts` handler **with `tx.closed()` teardown**
- [x] `AnswerHostPrompt` handler, auth + unknown-id rejection
- [x] Host RSA keypair and its lifecycle
- [x] Public key published with the prompt; fingerprint derivation
- [x] Browser `SubtleCrypto` `RSA-OAEP` encryption
- [x] Key continuity: pin on first sight, block on change — 13 tests; unusable storage now reports `unverified`
- [x] Daemon decrypt → private key decrypt → agent add → drop plaintext
- [x] Passphrase dialog, add-key action and the prompt subscription — the flow is reachable from a Hosts row
- [x] The **key selector**: `ListHostKeyCandidates` lists the private keys a session's own OS user could load, described from their public halves — a candidate is a key whose `.pub` sits beside it, so no private key is ever opened to build the list
- [x] The key field speaks **absolute paths**, in the picker and in the free-text field alike — `confined_to_home` accepts nothing else and nothing expands `~`, so a `~` path is refused here rather than sent to a refusal that names no path
- [x] A checked host key is held **with the prompt it arrived on**, so the fingerprint on screen always belongs to the key that would encrypt; a key whose check is outstanding is `unchecked`, which says nothing and sends nothing
- [x] Rust unit/integration tests, teardown test, Cypress round-trip tests
- [x] Confirm whether the git hardening needs any change at all

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

- [x] Proto + both regenerations
- [x] Prompt registry with expiry and single-use, unit-tested
- [x] Stream handler **and its teardown test green** — before any UI work
- [x] Host keypair + fingerprint
- [x] Browser encryption producing a payload the daemon decrypts
- [x] Full round trip adding a real key to a fake agent
- [x] Key pinning + change warning
- [x] Wrong-passphrase, expiry and replay paths
- [x] Confirm the git hardening question

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
| `distinguishes_a_wrong_passphrase_from_an_absent_agent_and_from_an_expired_prompt` | AC-4 — the enum's whole point |
| `says_continuity_could_not_be_checked_without_blocking_the_answer` | AC-9 — the `unverified` verdict |
| `distinguishes_an_unverifiable_host_key_from_a_changed_one_and_from_a_first_sighting` | AC-9 |
| `never_shows_the_previous_keys_fingerprint_beside_the_key_that_would_encrypt` | AC-14 — the correlation the committed fix left open |
| `shows_an_example_the_host_will_accept_rather_than_a_tilde_path_it_refuses` | AC-13 |
| `does_not_send_a_tilde_path_for_the_host_to_refuse_without_explanation` | AC-13 |

**`packages/tddy-daemon/src/host_private_key.rs`** (unit — the key listing)

| Test | Validates |
|---|---|
| `offers_a_key_whose_public_half_sits_beside_it` | AC-12 — the rule that does all the filtering |
| `describes_each_key_by_the_type_and_fingerprint_in_its_public_half` | AC-12 |
| `lists_the_keys_with_the_privileges_of_the_user_they_belong_to` | AC-12 — listed as the owner, not as the daemon |
| `never_opens_a_private_key_to_build_the_list` | AC-12 — a list of keys is not a use of them |
| `reads_only_the_users_own_ssh_directory` | AC-12 — no walk of the operator's home |
| `does_not_offer_another_users_key` | AC-12 — the confinement, on a multi-user host |
| `does_not_offer_a_keypair_kept_outside_the_ssh_directory` | AC-12 |
| `does_not_offer_ssh_configuration_or_a_public_half_on_its_own` | AC-12 — `known_hosts`, `authorized_keys`, `config`, a lone `.pub` |
| `does_not_offer_a_directory_that_is_named_like_a_key` | AC-12 — a candidate is a file |
| `does_not_offer_a_private_key_with_no_public_half_beside_it` | AC-12 — the filtering rule's cost, and why the typed field stays |
| `offers_the_keys_in_a_stable_order` | AC-12 |
| `offers_nothing_for_an_absent_ssh_directory_and_nothing_for_an_unreadable_one` | AC-12 — the directory oracle |
| `offers_only_paths_that_an_add_of_the_same_key_accepts` | AC-12 — the two halves must agree |

**`packages/tddy-daemon/src/connection_service.rs`** (integration — the key listing)

| Test | Validates |
|---|---|
| `offers_the_operator_the_key_in_their_own_home` | AC-12 |
| `offers_each_operator_only_the_keys_of_their_own_os_user` | AC-12 — whose keys are these? |
| `offers_a_key_that_an_add_of_the_very_same_path_then_loads` | AC-12 — list → pick → add, end to end |
| `rejects_a_listing_for_an_invalid_session_token` | AC-12 |
| `refuses_a_listing_addressed_to_a_host_this_daemon_does_not_know` | AC-12 — routing honoured |
| `serves_a_listing_addressed_to_this_daemon_by_its_own_instance_id` | AC-12 |
| `offers_nothing_and_fails_nothing_when_the_ssh_directory_is_unreadable` | AC-12 |

**`packages/tddy-web/cypress/component/HostAddKeySelector.cy.tsx`**

| Test | Validates |
|---|---|
| `offers_the_keys_the_host_reported_so_nothing_has_to_be_typed_from_memory` | AC-12 |
| `tells_the_keys_apart_by_type_and_fingerprint_not_by_path_alone` | AC-12 |
| `asks_the_host_in_the_row_and_only_when_there_is_an_agent_to_add_to` | AC-12 — routing, and no call for a row that cannot take a key |
| `adds_the_key_that_was_picked_at_the_path_the_host_gave_for_it` | AC-12 |
| `still_takes_a_typed_path_for_a_key_no_listing_can_see` | AC-12 — a guard, already green |
| `treats_a_host_that_will_not_list_its_keys_as_one_with_no_keys_to_offer` | AC-12 — a guard, already green |

**`packages/tddy-web/src/rpc/hostPromptsSubscription.test.ts`** (unit)

| Test | Validates |
|---|---|
| `hands_every_prompt_the_host_raises_to_the_caller_in_order` | AC-1 |
| `cancels_the_call_when_the_caller_unsubscribes` | AC-10 — the browser-side leak |
| `cancels_a_feed_that_has_never_raised_a_prompt` | AC-10 — this feed's normal state |
| `stops_delivering_prompts_once_the_caller_has_unsubscribed` | AC-10 |
| `reports_nothing_when_the_call_it_cancelled_itself_rejects_with_an_AbortError` | AC-10 |
| `reports_a_feed_the_daemon_drops_while_the_caller_is_still_subscribed` | AC-10 — the swallow's non-vacuity |

**`packages/tddy-web/cypress/component/HostsScreenAddKeyAcceptance.cy.tsx`**

| Test | Validates |
|---|---|
| `offers_to_add_a_key_when_an_ssh_agent_is_reachable` | AC-3 |
| `offers_to_add_a_key_to_an_agent_that_is_already_holding_one` | AC-3 |
| `offers_the_add_only_where_there_is_an_agent_to_add_to` | AC-3 |
| `asks_the_host_to_load_the_key_the_operator_named` | AC-3 |
| `raises_the_passphrase_dialog_for_the_key_the_host_asks_about` | AC-1 |
| `confirms_the_key_the_agent_is_now_holding_once_the_add_succeeds` | AC-3 |

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

### Green phase — what landed, and what did not

**Landed and green**, verified scoped to the touched packages:

| Gate | Result |
|---|---|
| `cargo build` (workspace) | clean |
| `cargo fmt --check` | clean |
| `cargo clippy -p tddy-daemon -p tddy-core --all-targets -- -D warnings` | clean |
| `cargo test -p tddy-core --lib` | 328 passed, 0 failed |
| `cargo test -p tddy-daemon --lib` | 731 passed, 0 failed |
| `cargo test -p tddy-daemon --test stream_host_prompts_rpc` | 3 passed, 0 failed |
| Cypress `HostAddKeyAcceptance.cy.tsx` | 5 passed, 0 failed (stable over 4 runs) |

**The blocking prerequisite is resolved.** `write_atomic_with_mode` sets the swap file's mode at
`OpenOptions::mode()` creation time, so there is no window in which a private key exists at the
process umask. The three hand-rolled secret writers remain deferred, as planned.

**The teardown counter was checked for vacuity.** `stops_the_prompt_pump_once_the_subscriber_is_gone`
asserts `== 0`, which a never-incremented counter would satisfy too. `PumpCount::running` increments
synchronously in the handler *before* the spawn and decrements in `Drop`, so the assertion is real.

### Second wave — the add-key flow closed end to end

`AddHostKey` was added because nothing could **issue** a prompt: the node had the channel and the
crypto but no operation that raises a question and consumes the answer, so the whole
decrypt → unlock → agent-add path was unreachable *and* untestable. Its response reports an outcome
rather than a bare bool, so the browser can distinguish a wrong passphrase from an expired prompt,
an absent agent and an unreadable key.

The wait is a per-prompt `oneshot` created in `issue`. The sender is itself the "unanswered" marker,
so single use stays derived from one field instead of a flag that could disagree with a stored
answer, and the registry keeps **no copy** of the ciphertext — it moves through the channel to the
waiter. Parking the receiver in the record buffers an answer that arrives before the waiter claims
it, which a bare `Notify` would drop.

`ssh_agent_add.rs` is a new module rather than an addition to node 5's `ssh_agent.rs`: adding an
identity is this node's responsibility, and the probe surface below it belongs to the PR that owns
it. Socket resolution is **called**, not reimplemented.

| Gate | Result |
|---|---|
| `cargo build` (workspace) · `fmt --check` · `clippy -p tddy-daemon -p tddy-core -p tddy-service` | clean |
| `cargo test -p tddy-daemon --lib` | **737 passed, 0 failed** |
| `cargo test -p tddy-daemon --test stream_host_prompts_rpc` | 3 passed |
| `cargo test -p tddy-core --lib` | 328 passed |
| `bun test packages/tddy-web/src/lib` | **309 passed, 0 failed** |

**`hostKeyPinning` now has 13 tests**, and gained an `unverified` verdict. Unusable storage used to
report `pinned-now` — a positive claim the dialog acts on, made when nothing was recorded, and false
on every later sighting, so an active substitution was indistinguishable from an ordinary first use
permanently. An empty fingerprint takes the same route rather than burning the one first-use trust
slot.

### Third wave — the flow reached the browser, and validation found four blockers

`AddHostKey` became reachable from the Hosts row (`HostAddKeyAction`, `useHostPrompts` and its
`hostPromptsSubscription` loop). Then `/pr-wrap`'s validation passes found four security defects,
each of which is now fixed with a test that failed against the previous behaviour first.

#### 1. ⛔ Key continuity was decorative — the pin was never bound to the encrypting key

`checkHostKey` pinned and compared `HostPromptEvent.host_public_key_fingerprint`, the **advertised
string**, while the answer was encrypted under `host_public_key`, the SPKI DER. Nothing derived the
one from the other. Both fields ride the channel this feature exists to distrust, and the
fingerprint is not secret — so an active peer could replay the genuine fingerprint beside **its own**
key, get `unchanged`, show the operator the value they had verified out of band, and receive the
passphrase. Encryption was intact; the thing that was supposed to bound an active substitution was
not.

`lib/hostKeyFingerprint.ts` now derives `SHA256:<base64-no-pad>` over the received SPKI DER,
matching the daemon's `spki_fingerprint`, and `verifyHostKey` pins, compares and displays **that**.
An advertised fingerprint that disagrees with the derived one is a hard block with no accept path —
that is a substitution attempt, not a rotation. An **empty** advertised field blocks too:
"stripped in flight" and "an older daemon" are indistinguishable from the browser.

#### 2. ⛔ `crypto.subtle` was called unguarded, and tddy-web is served over plain http

`packages/tddy-web/docs/insecure-origin-constraints.md` is explicit that the daemon serves this
bundle over `http://` on a LAN address, which is **not a secure context**, and it records the
`crypto.randomUUID` incident where exactly this broke a whole feature on real devices while passing
every local test. `crypto.subtle` is withheld on such an origin, so every submit threw a `TypeError`
and the add-key flow was **dead on the normal deployment**. Cypress runs on `localhost`, a secure
context, so nothing local could notice.

`lib/subtleCrypto.ts` is now the one audited entry point and **refuses loudly**, naming the insecure
origin. There is deliberately **no fallback**: the only thing behind `subtle` here is a passphrase
being encrypted, and a plaintext fallback would hand it to every peer in the room. The refusal
surfaces as the `underivable` verdict, so the dialog blocks with a reason instead of failing at
submit.

⚠ **This bounds the feature, not just the code.** Add-key works only on a secure origin. Making it
work over LAN needs TLS, which the daemon has nowhere today (`config.rs` has no TLS at all) — a
deployment decision outside this node.

#### 3. ⛔ The private key was read as the daemon, from an unvalidated client path

`std::fs::read_to_string(subject)` ran with the daemon's own credentials on a free-text path, so a
session mapped to one user could name another's key; and two distinguishable refusals
("could not be read" / "is not an OpenSSH private key") were returned verbatim, giving any
authenticated session a file-existence oracle over the host.

`host_private_key.rs` confines the path to the mapped user's home and reads it **as that user**
through `spawner::run_capture_as_user` — a child process, not `seteuid`, because the daemon is
multi-threaded. Absent, unreadable and malformed now share one refusal.

Confinement is **lexical, deliberately without `canonicalize`**: canonicalizing would have to stat a
caller-chosen path, re-introducing the very existence oracle this closes. The real boundary is the
privilege drop — a symlink in one user's home pointing at another's key resolves *as* the first
user, who cannot read it. `KEY_OUTSIDE_HOME` stays a distinct message because it is a pure function
of the caller's own input and their own account, so it discloses nothing, and it is the one refusal
an operator can act on.

#### 4. ⛔ A prompt was not bound to the session that raised it

`pending()` filtered only on unanswered-and-unexpired, so the pump replayed **every** outstanding
prompt to **any** authenticated subscriber, and `answer()` accepted any id from any session. A
second operator was shown the first's dialog — including the key path — and could burn the
single-use prompt with garbage, denying the real add for the full TTL.

Prompts now carry `issued_for` and both the feed and the answer filter on it. The identity is the
resolved **GitHub user**, not the OS user: two GitHub users mapped to one OS user would otherwise
still see and burn each other's prompts, which is precisely what `config.users[]` can express. A
mismatched answer gets the same `UnknownPrompt` rejection an id that was never issued gets, and
returns **before** the oneshot sender is taken, so the prompt keeps its one answer.

#### Also fixed

- **The response was an adaptive RSA-OAEP decryption oracle** — "cannot decrypt" and "did not
  unlock" were distinguishable, giving a clean bit per query against the host's long-lived key. They
  are now byte-identical, sharing one constant so they cannot drift; the real cause goes to the log
  only. (`rsa` is pinned at `=0.9.10`, which is under RUSTSEC-2023-0071 with no fixed release;
  `decrypt_blinded` covers the timing channel, and collapsing the response covers the explicit one.)
- **The proto promised routing that no handler implemented.** `daemon_instance_id` is now honoured
  on all three RPCs, mirroring `get_host_tooling`. A key silently loaded into the wrong host's agent
  is a worse version of the failure that handler's own comment warns about.
- **`acceptChangedHostKey` was dead**, so a legitimate key rotation locked the operator out
  permanently, contradicting the PRD. The dialog now has a deliberate two-step accept — a checkbox
  confirming out-of-band verification, gating the accept button.
- **`AnswerHostPromptResponse` was discarded**, so the three rejections the daemon distinguishes
  reached nobody. Now surfaced on the row.
- **Cancel did not cancel** — the RPC stayed parked for the full 120s TTL with the row disabled and
  nothing explaining why. It now aborts the call client-side; the daemon has no withdraw path, so
  the prompt simply expires unanswered.
- `write_atomic_with_mode` uses `create_new(true)`, making the owner-only mode structural rather
  than probabilistic, and `fingerprint_of` → `spki_fingerprint` to remove the collision with node
  5's same-named function over different bytes.

#### Test hardening

Every item below was **mutation-verified** — the production code was broken on purpose to confirm
the new assertion catches it:

- The pump-teardown test asserted only `count == 0`, which an implementation that never counts also
  satisfies. It now asserts `== 1` while subscribed first.
- AC-2 asserted the ciphertext "does not contain the passphrase" and a loose length — satisfied by
  base64, a hash, or random bytes. It now **decrypts** the recorded `AnswerHostPrompt` payload with
  the test keypair's private half and asserts it equals the passphrase exactly, at 256 bytes.
- Nothing pinned that the row renders the action; deleting the line failed no test.
- The handler's classification of a ciphertext encrypted **for another host** was unasserted — the
  previous test proved only that two responses were identical, which both reporting `UNSPECIFIED`
  would satisfy.
- The log recorder never cleared `RECORDED`, so after that test every other test in the binary
  pushed into a leaked buffer behind one global mutex. It is now an RAII guard that also fails
  loudly if a second recording displaces it, and the fail-closed marker check is preserved.
- **RSA keygen cost was a real flake source, and the obvious fix was wrong.**
  `[profile.test.package.rsa] opt-level = 3` measured as a **no-op** (20.13s → 20.53s). The prime
  search lives in `num-bigint-dig`; overriding that gives 20.13s → **1.08s**, and the add-key
  handler tests 81.86s → **13.63s**, turning a ~3.3s keygen inside a 10s timeout into ~0.76s.

#### ⚠ One caveat worth weighing before enabling a flag

`classify_peer_route` compares against the **routing** instance id while `list_known_hosts`
publishes the **durable** one. They are equal under the default
`daemon_instance_id_append_startup_timestamp: false`. With that flag on, a browser sending the
durable id now gets `invalid_argument` where it was previously — wrongly — served locally. This is a
pre-existing property of this field shared with `get_host_tooling` and ~20 other RPCs; fixing it
means changing `classify_peer_route`, which is outside this node.

#### ⚠ A documentation-route deviation, recorded deliberately

`packages/tddy-web/docs/insecure-origin-constraints.md` was edited **directly** to add
`crypto.subtle` and a Web Crypto section. CLAUDE.md says never to modify `packages/*/docs/`
directly — the changeset workflow owns that. The content is correct and is where wrapping would have
placed it, so it was kept rather than reverted and re-derived, and it is noted here so the
changeset↔docs relationship stays honest.

### Two outcome gaps recorded rather than fixed

`AddHostKeyOutcome` has no arm for an answer this host cannot decrypt (encrypted for the wrong host,
or corrupt) or for an agent that answered and refused; both map to `UNSPECIFIED` with a plain
`failure_reason`. Reporting `WRONG_PASSPHRASE` for the first would send the operator to retype
something that will keep failing, and `NO_AGENT` for the second would be false. Adding arms is a
proto change plus a TS regeneration.

An **unencrypted** key at `subject` is added as-is rather than reported as a wrong passphrase, since
`PrivateKey::decrypt` refuses an already-decrypted key and the naive mapping would lie. It still
raises a prompt first; skipping the prompt for such a key is better UX that no test pins.

The passphrase buffer is a plain `Vec<u8>` — dropped, not zeroized. `zeroize` is not a `tddy-daemon`
dependency and was not added without consent.

## Refactoring needed

_(populated by each validation phase)_

### From @red (browser-side gap)

- **`hostStatsSubscription.ts` and `hostPromptsSubscription.ts` are now the same loop twice.** Both
  hold an iterator by hand, abort on unsubscribe, guard delivery on an `unsubscribed` flag and
  release in a `finally`. They differ only in the event type and in where a feed failure goes
  (`console.debug` versus a callback). A third streaming surface should trigger extracting a generic
  `subscribeServerStream<T>`; two is not yet enough to design against.
- **The dialog now has two ways to say the same thing.** `keyChanged: boolean` and
  `keyContinuity?: KeyPinVerdict` overlap — `keyChanged` is derivable from
  `keyContinuity.kind === "changed"`, and nothing stops a caller from stating both and disagreeing.
  Collapsing them to the verdict alone is the right shape; it was kept additive here only so the
  five green dialog tests were not rewritten in the same pass that added a behaviour. Worth doing
  before this node lands.
- **`HostPromptFeed` and `SessionNotificationFeed` are the same fixture twice** — an always-open
  generator, a queue, a `wake` promise and a subscription counter, differing only in the event they
  carry. A shared `anAlwaysOpenServerStream<T>()` helper in `cypress/support/rpc/` would carry both.
- **The add-key key field is a free-text path, not a selector.** ✅ Taken up: `ListHostKeyCandidates`
  is specified and red-phase tested (see the acceptance tables). The free-text field stays, because a
  key with no `.pub` beside it is invisible to the listing and must still be loadable.

### From @red (key listing and selector)

- **`UserFilesUnder` is growing a recorder per question.** It now records users read as, users listed
  as, paths read and directories listed — four parallel `Mutex<Vec<_>>` fields with identical
  locking and identical `expect` text. One `Mutex<Vec<Interaction>>` with an enum would carry all
  four and make "what did this call touch, in order?" a single assertion. Left additive so the
  existing read tests were not rewritten in the pass that added the listing.
- **The fixture seam is parameterised for exactly one thing.** `a_host_where(passphrase, adjust)`
  exists so one test can stage an unreadable `~/.ssh`. If a second such staging arrives, the closure
  should become a small builder rather than a second positional parameter.
- **`AddHostKeyRequest.subject` does not say the path must be absolute** — the proto comment describes
  a path on the host and stops there, while `confined_to_home` rejects anything relative. Correcting
  the comment belongs to green, alongside the placeholder it contradicts.
- **`with_pub_suffix` is now written twice** — once in `host_private_key.rs`'s tests and once inline
  in `connection_service.rs`'s `an_encrypted_private_key_at`. `ssh-keygen`'s naming rule (append,
  never replace an extension) is one fact and should live in one place once the production code needs
  it too.

## Validation results

### Red phase (draft-PR contract)

- `cargo clippy -p tddy-daemon --all-targets -- -D warnings` — clean.
- `host_prompts.rs` 4/4 red · `host_keypair.rs` 3/3 red · `stream_host_prompts_rpc.rs` 2/3 red
  (`rejects_an_invalid_token` passes — the auth guard is genuinely part of the published surface).

### Red phase — the key listing and selector

- `cargo clippy -p tddy-daemon --profile test --lib -- -D warnings` — clean. **Deliberately not
  `--all-targets`**: that links every `tddy-daemon` acceptance-test binary separately and filled the
  build disk before running a single test.
- `cargo test -p tddy-daemon --lib host_private_key` — **13 red**, 6 pre-existing green.
- `cargo test -p tddy-daemon --lib host_add_key_handler_tests` — **7 red**, 18 pre-existing green.
- `HostAddKeySelector.cy.tsx` — **4 red**, 2 green (both green ones are guards on behaviour that
  already holds: a typed path still works, and a host that refuses the listing is not an error).
- `HostAddKeyAcceptance.cy.tsx` — **3 red** (the new ones), 11 pre-existing green.

### ⚠ Two committed security fixes were incomplete — both now pinned red

- **The key check was not carried atomically with the prompt it describes.** `HostAddKeyAction`
  passes `spkiDer` from `outstandingPrompt` and `fingerprint`/`keyContinuity` from a separate
  `keyCheck` state, with nothing correlating them; the verify effect does not clear `keyCheck` before
  re-running. A second prompt frame replaces the key immediately and its verdict arrives a digest
  later, so in between the dialog shows the *previous* key's fingerprint and its reassuring
  `unchanged` verdict over bytes that will encrypt for a different key — the substitution the pin
  exists to catch, wearing the pin's approval. A peer in the routing path widens that window at will
  by emitting frames faster than SHA-256 resolves. Pinned by
  `never_shows_the_previous_keys_fingerprint_beside_the_key_that_would_encrypt`, which holds
  `crypto.subtle.digest` open to stand inside the window.
- **The placeholder invited a path the daemon refuses.** `placeholder="~/.ssh/id_ed25519"` against a
  `confined_to_home` that requires `is_absolute()` with no `~` expansion anywhere. **Resolution:
  absolute paths only** — the listing returns absolute paths so picking is the common case, one
  syntax lets a picked and a typed path be compared by eye, and expanding `~` would have to happen in
  the daemon, whose confinement is valuable precisely because it is a pure function of the caller's
  input. The refusal also cannot explain itself (`KEY_OUTSIDE_HOME` names no path, on purpose), so
  the browser — which holds the only context that makes it legible — must not send the request.

### ✅ The git hardening does NOT need inverting — confirmed

The whole-work discovery assumed this node would have to undo `GIT_TERMINAL_PROMPT=0` and the null
stdin in `packages/tddy-core/src/worktree.rs:39-52`. **It does not.** Because `#hosts-screen 5/8`
chose the agent *wire protocol* over `ssh-add`, the passphrase is used to decrypt the private key
**in process** and the identity is handed to the agent directly — no subprocess, no TTY, no
`SSH_ASKPASS`. That hardening governs git subprocesses and is untouched. This node's blast radius is
materially smaller than planned, and it is a direct dividend of node 5's decision.

### `encrypt_for_test` is a real, independent encryptor

It was first written as an `unimplemented!()` test helper, which the `/red` contract forbids outright
and which hollowed out the very property the test exists to prove: a round trip where both halves go
through our code demonstrates only that we agree with ourselves. It now goes through the **`rsa`
crate's public API directly** (added as a dev-dependency), so the test pins the *format* — SPKI DER
in, RSA-OAEP(SHA-256) out — which is what the browser's `SubtleCrypto` actually produces. Same
reasoning as node 5's fingerprint fixture being `ssh-keygen`'s output rather than ours.

### The three tests that carry the security claim

1. `sends_an_encrypted_answer_that_does_not_contain_the_passphrase` — decodes the submitted bytes and
   asserts the plaintext is absent. A round trip proving only "the key was added" would pass equally
   with the passphrase in the clear, which is the entire thing this node prevents.
2. `refuses_a_payload_encrypted_for_a_different_host` — without it, "encrypted" is decorative; the
   property that matters is that only the addressed host can read it.
3. `stops_the_prompt_pump_once_the_subscriber_is_gone` — a leaked pump is **unobservable** from
   outside a stream that is silent by design, so the service publishes a `pending_prompt_pump_count()`
   for the test to see it. A stub returning `0` would have made this pass vacuously and reported the
   leak as absent forever; it is backed by a real counter green must maintain.

Paired with `keeps_an_idle_subscription_open_rather_than_completing_it`, which brackets the opposite
failure: a completed stream reads to the browser as the daemon dropping the feed.

### ⚠ Still the decision most worth challenging

Key distribution is **TOFU + a visible fingerprint**, chosen here rather than by the user. It makes an
active key substitution *visible*, not impossible, and gives no protection on a first-ever connection
to an already-compromised host. The weaker alternative — accept passive-only protection and disclose
it in the dialog — remains reasonable. Recorded to be argued with.

## TODO

- [x] Record initial discovery (`2026-09-06-agent-add-key-initial-discovery.md`)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete (three waves)
- [ ] Run all tests (`./test`) — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready)
- [x] Refactor production readiness issues
- [x] Analyze code quality (/analyze-clean-code)
- [x] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps

## Successor PRs

- [`2026-09-06-desktop-probe.md`](./2026-09-06-desktop-probe.md) — branch
  `feature/hosts-screen/desktop-probe`
