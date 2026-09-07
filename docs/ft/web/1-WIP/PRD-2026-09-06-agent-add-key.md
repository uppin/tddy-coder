# PRD — Load a key into a host's ssh-agent, with an end-to-end encrypted passphrase

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (prompt UI) + `daemon` (prompt channel, crypto, agent add)
**Stack:** `#hosts-screen` node 6 of 8
**Branch:** `feature/hosts-screen/agent-add-key` → base `feature/hosts-screen/agent-keys`

## Affected features

| Document | Relationship |
|---|---|
| [`PRD-2026-09-06-agent-keys.md`](./PRD-2026-09-06-agent-keys.md) | Supplies the agent client and the key list this node mutates |
| [`docs/ft/web/projects-screen-multi-host.md`](../projects-screen-multi-host.md) | States the common-room trust model this node's crypto is answering |

## Summary

Let an operator **load a key into a host's ssh-agent from the browser**. The host asks for the key's
passphrase, the prompt appears in tddy-web, and the answer travels back **encrypted under that host's
public key** — so neither the LiveKit common room nor any forwarding daemon ever sees it in plaintext.

The passphrase is never persisted, never logged, and is dropped as soon as the key is decrypted.

## Background

Two things make this the hardest node in the stack.

**Nothing in tddy prompts for a secret from the server.** The one server-asks-UI mechanism is the ACP
bidi stream (`packages/tddy-web/src/components/chat/useAcpSession.ts`), and it was considered and not
chosen. The existing passphrase dialogs (`ScreenSharingPassphraseDialog`, `VncPassphraseDialog`) run in
the **opposite** direction — the UI decides to ask before making a unary call.

**And the daemon is deliberately hardened against exactly this.**
`packages/tddy-core/src/worktree.rs:39-52` closes stdin and sets `GIT_TERMINAL_PROMPT=0` so that "a
missing key/passphrase or credential prompt fails fast instead of blocking forever — a headless daemon
has no TTY to answer such a prompt."

## Proposed changes

### The channel: server-stream + unary reply

- **`StreamHostPrompts`** — a server-streaming RPC on which the daemon pushes a pending question
  (a prompt id, the host, the key being unlocked, and the host's public key).
- **`AnswerHostPrompt`** — a unary RPC carrying the prompt id and the **encrypted** answer.

This mirrors `StreamWorktreeStats` + `CalculateWorktreeSize`, already used by `useWorktreeStatsStream`.
The unary reply can be transport-restricted following the `mint_local_token` precedent.

### The crypto

- The host holds an **RSA keypair**; its public half is published with the prompt.
- The browser encrypts the passphrase with **`RSA-OAEP` via `SubtleCrypto`** — a platform API, **no new
  web dependency**. A passphrase fits comfortably in a 2048-bit OAEP payload (~190 bytes), so **no
  hybrid envelope is needed**.
- The daemon decrypts in-process, uses the passphrase to decrypt the private key, adds the identity to
  the agent, and **drops the plaintext immediately**.

### Key distribution — the honest limit, and what this PRD commits to

Encryption under the host's public key defeats a **passive** relay. It does **not** by itself defeat an
**active** peer: the client learns the public key over the same channel, and the trust model says
plainly that the common room is "a trusted peer group, **not a cryptographically authenticated one**".
A hostile participant advertising itself as `daemon-<instance_id>` could publish its own key.

**This PRD commits to key continuity plus a visible fingerprint** — SSH's own TOFU model:

- A host's public key is **pinned on first sight**.
- A **changed** key blocks the flow with a loud warning; the operator must accept the new key explicitly.
- The prompt dialog **always displays the key fingerprint** and names the host, so an operator can
  verify out of band.

> ⚠ **This is the one design choice in the stack that a reviewer should challenge deliberately.** The
> alternative — accept passive-only protection and disclose it in the dialog — is simpler and weaker.
> The choice is recorded here to be argued with, not assumed.

### Does the git hardening actually need inverting?

**Probably not, and that is a direct benefit of node 5's decision.** Because the daemon speaks the
agent protocol directly and decrypts the private key **in-process**, it never runs `ssh-add` and never
needs a TTY or `SSH_ASKPASS`. The `GIT_TERMINAL_PROMPT=0` / null-stdin hardening governs *git
subprocesses* and is untouched by this flow. **Confirm in the red phase**; if it holds, this node's
blast radius is much smaller than first assumed.

## Impact analysis

### Technical

- ⚠ **The prompt stream is silent almost all the time — the leaking case.**
  `packages/tddy-codegen/docs/server-streaming.md` requires a handler whose stream can be silent to
  `tokio::select!` on `tx.closed()` as well as breaking on send error, or the handler task leaks one
  per subscription forever. `stream_host_stats` escapes this only by emitting unconditionally.
  **This node must implement and test that teardown**, mirroring `pump_rooms`
  (`packages/tddy-daemon/src/livekit_rooms_stream.rs`) and its pinned regression test.
- A **third** new external crate (an RSA implementation) joins node 5's two.
- Prompts must **expire**. An unanswered prompt cannot pin a pending operation indefinitely.
- A prompt must be **single-use** — answering it twice must not add a key twice or leak a retry oracle.

### Security

- Plaintext passphrase exists only inside the daemon process, for the duration of the decrypt.
- **Never** logged, **never** written to disk, **never** placed in daemon state beyond the call.
- The encrypted answer is opaque to the common room and to any forwarding daemon.
- Residual risk: an active peer substituting its public key, bounded by pinning + fingerprint display.

### User

- A host row gains an "add key" action, a key selector fed by `ListHostKeyCandidates`, and a
  passphrase dialog naming the host and its key fingerprint. The selector offers absolute paths, and
  the free-text field beside it — the only way to reach a key with no `.pub` — speaks the same
  syntax.

## Acceptance criteria

- [ ] **AC-1** Starting an add-key flow surfaces a prompt in tddy-web naming the host and the key.
- [ ] **AC-2** The answer leaves the browser **encrypted**; the plaintext passphrase never appears in
      any request payload.
- [ ] **AC-3** A correct passphrase results in the key being held by that host's agent, visible in the
      node 5 key list.
- [ ] **AC-4** An incorrect passphrase reports a failure and adds nothing.
- [ ] **AC-5** The passphrase is never logged and never written to disk.
- [ ] **AC-6** A prompt expires unanswered, releasing the operation.
- [ ] **AC-7** A prompt can be answered only once.
- [ ] **AC-8** The prompt dialog displays the host's public-key fingerprint.
- [ ] **AC-9** A **changed** host key blocks the flow with an explicit warning rather than proceeding.
- [ ] **AC-10** When the subscriber goes away, the daemon's prompt-stream task is torn down — no leak.
- [ ] **AC-11** `AnswerHostPrompt` rejects an invalid `session_token` and an unknown prompt id.
- [ ] **AC-12** An operator **picks** a key from the ones that host reports for their own OS user,
      rather than recalling a path — and every path offered is one `AddHostKey` accepts. The listing
      is built from public halves only: no private key is opened to describe a candidate, and a
      user's absent, empty and unreadable `~/.ssh` are one answer.
- [ ] **AC-13** The key field never invites a path the host is bound to refuse. `confined_to_home`
      takes absolute paths and nothing expands `~`, so both surfaces speak absolute paths and a
      relative one is refused here, where the reason can still be stated.
- [ ] **AC-14** The fingerprint the dialog displays and pins always belongs to the key that will
      encrypt the answer — including when a second prompt replaces the key while the dialog is open.

## Out of scope for this node

Removing a key from an agent. Generating a key. Expanding `~` on the host — see AC-13 for why that
resolution was rejected rather than deferred. Persisting a passphrase in any form (explicitly
rejected). Changing the git remote hardening unless the red phase proves it necessary. VNC/RDP
(nodes 7–8).

## Successor PRs

- `feature/hosts-screen/desktop-probe` — VNC and RDP reachability per host.
