# 2026-09-07 — The add-key action, the passphrase dialog and the pin behind it
**Type:** Feature

`HostAddKeyAction` (mounted by `HostRowSshAgent`), `HostPassphraseDialog`, `useHostPrompts` /
`hostPromptsSubscription`, and the three libs the crypto lives in — `hostKeyFingerprint.ts`,
`hostKeyPinning.ts`, `encryptForHost.ts`, all reached through `subtleCrypto.ts`. Full account in
[`hosts-screen.md`](../hosts-screen.md) §§ *Adding a key to a host's agent*, *Host key trust*, *The
passphrase dialog*, *Encrypting on a plain-http origin* and *Picking a key instead of recalling one*.

`AddHostKey` blocks for as long as the add takes, so **one component issues the call and renders the
dialog that call is waiting on**. The action is offered only where an agent answered — an unreachable
agent needs starting, not a key — using the same outcome-first guard `HostRowSshAgent` applies, since
a proto3 outcome this bundle cannot name must never read as a reachable agent. The report comes from
`AddHostKeyOutcome` per arm, not from `added`, because the enum exists precisely so a daemon with
nothing to say still distinguishes the failures an operator acts on differently.

**The fingerprint is derived here, never taken from the wire.** `hostKeyFingerprint.ts` computes
`SHA256:<base64-no-pad>` over the received SPKI DER — matching the daemon's `spki_fingerprint` — and
that is what is shown and pinned. The prompt's advertised fingerprint string rides the same
unauthenticated channel and is **not secret**, so pinning it made continuity decorative: an active
peer replays the genuine string beside its own key, the check reports `unchanged`, the operator
recognises the value they verified out of band, and the passphrase is encrypted to the peer. A prompt
whose two halves describe different keys is refused outright with no accept path, because a host
describing its own key gets it right. An **empty** advertised field blocks too — "stripped in flight"
and "an older daemon" are indistinguishable from here.

**`KeyPinVerdict` has six arms, and the two quiet ones carry the design.** `unverified` exists so a
missing conclusion is never reported as a positive one: unusable storage degrading to `pinned-now`
would claim "first sighting, key recorded" when nothing was recorded — false on every later sighting
too, leaving an active substitution indistinguishable from ordinary first use *permanently*. It warns
and does not block, because a browser that will not store a pin has caught nobody doing anything.
`unchecked` is the caller's in-flight state, never returned by `verifyHostKey`, and it blocks
silently: `unverified` would claim a check was attempted, `unchanged` would claim continuity nobody
established, and a key whose digest is not back is a key nothing may be encrypted to.

**A checked key is held with the prompt it arrived on, compared by object identity — not by
`prompt_id`, because two frames can share an id and carry different keys.** Only a pair whose prompt is
the one in hand is shown. Held as two pieces of state, the frame between a new key arriving and its
digest resolving shows the *previous* key's fingerprint and its reassuring verdict beside bytes that
would encrypt for somebody else, and a peer in the routing path widens that window at will by emitting
frames faster than SHA-256 resolves. Pinned by a spec that holds `crypto.subtle.digest` open to stand
inside it.

`acceptChangedHostKey` is a deliberate **two-step**: a checkbox confirming out-of-band verification
gates the accept button. It was dead code at one point, which left a legitimate rotation locking the
operator out of their own host permanently — the daemon regenerating `host-prompt-key.pem` is enough to
cause it.

**Encryption happens inside the dialog**, so the plaintext exists only in that component's state and
every path out carries ciphertext; a caller handed the passphrase would be one `console.log` away from
undoing the feature. `keyContinuity` is the only thing the dialog is told about the pin — a
`keyChanged: boolean` stood beside it while the verdict had no reader, and two props encoding one fact
can disagree.

⚠ **`crypto.subtle` was called unguarded, and this bundle is served over plain http.** Every submit
threw a `TypeError` on the normal deployment while passing every local test, because Cypress runs on
`localhost`, a secure context. `subtleCrypto.ts` is now the one audited entry point and **refuses
loudly, naming the origin**, surfacing as the `underivable` verdict so the dialog blocks with a reason
instead of failing at submit. There is deliberately **no fallback and must not be one**: the only
thing behind `subtle` here is a passphrase, and a plaintext fallback would hand it to every peer in the
room. This bounds the feature — add-key works only on a secure origin, and a LAN address needs TLS the
daemon has nowhere today.
[`insecure-origin-constraints.md`](../insecure-origin-constraints.md) was edited **directly** to record
that, a knowing deviation from the changeset-only rule for `packages/*/docs/`, kept because the content
is correct and is where a wrap would have put it.

**Both key fields speak absolute paths, and `~` is expanded nowhere** — not here, which does not know
the host's home, and not in the daemon, whose confinement is valuable precisely because it is a pure
function of the caller's input. Its refusal names no path, so the browser holds the only context that
makes a tilde path legible and therefore refuses it rather than sending it. The picker offers the
host's listed keys by **type and fingerprint** (two paths can hold one key), and the free-text field
stays beside it because a key with no `.pub` is invisible to a listing that never opens private keys.

**Cancel now cancels** — it aborts the call client-side instead of leaving the row disabled for the
prompt's full 120 s TTL with nothing explaining why; the daemon has no withdraw path, so the prompt
expires unanswered. And `AnswerHostPromptResponse` is no longer discarded, so the three rejections the
daemon distinguishes reach the row.

**Deferred**, recorded rather than done: `hostStatsSubscription.ts` and `hostPromptsSubscription.ts`
are now the same read loop twice — abort on unsubscribe, guard delivery on a flag, release in a
`finally` — but extracting a generic `subscribeServerStream<T>` would edit node 3's file, and the real
scope is ~18 app-wide consumers rather than these two. `HostPromptFeed` and `SessionNotificationFeed`
are likewise the same Cypress fixture twice, and would share an `anAlwaysOpenServerStream<T>()` helper.

⚠ **Nothing in `src/` mounts `HostRowTooling`**, so nothing mounts the section this action lives in
either. It is wired, covered and reachable over the wire, and no operator can open a screen that shows
it. That limit is inherited from node 4, not introduced here.

Node 6 of the `#hosts-screen` stack — [PR #458](https://github.com/uppin/tddy-coder/pull/458).

See [`hosts-screen.md`](../hosts-screen.md) and
[`insecure-origin-constraints.md`](../insecure-origin-constraints.md).
