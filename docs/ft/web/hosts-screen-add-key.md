# Loading a key into a host's ssh-agent

An operator adds a private key to a host's ssh-agent **from the browser**. The host asks for the
key's passphrase, the question appears in tddy-web, and the answer travels back **encrypted under
that host's own public key** — so neither the LiveKit common room nor any daemon forwarding the call
ever sees it in plaintext.

The passphrase is never persisted, never logged, never written to disk, and is dropped as soon as the
key is unlocked. There is no "remember this passphrase", by decision rather than by omission.

## Motivation

[The tooling row](./hosts-screen-tooling.md) can already say that a host's agent is reachable and
holding nothing — which is exactly the state in which a session on that host cannot clone or push.
Telling an operator that and offering them nothing to do about it means opening a terminal on the
machine, which is the thing a fleet screen exists to avoid.

Two facts made this the hardest thing on the screen to build. **Nothing in tddy had ever asked the UI
for a secret**: the existing passphrase dialogs run the other way round, with the UI deciding to ask
before it makes a call. And **the daemon is deliberately hardened against interactive prompts** — it
closes stdin and sets `GIT_TERMINAL_PROMPT=0` so a credential prompt fails fast instead of blocking a
machine with no TTY to answer it.

That hardening is untouched, and it is a direct dividend of the agent read side choosing the agent
*wire protocol* over `ssh-add`: the passphrase unlocks the key **in the daemon's own process** and the
identity is handed to the agent directly. No subprocess, no TTY, no `SSH_ASKPASS`.

## What an operator does

1. On a Hosts row whose **agent answered**, an add-key control is offered. On a row whose agent did
   not answer, it is not: that host needs an agent started, not a key loaded.
2. They **pick a key** from the ones that host reports for their own OS user — each shown by type and
   fingerprint, not by path alone — or type an absolute path for a key no listing can see.
3. The host raises a question, and a dialog appears naming **the host**, **the key** and the host's
   **public-key fingerprint**.
4. They type the passphrase. It is encrypted in the browser before it leaves.
5. The row reports what happened: the key was added and is now held by that agent, or one of the
   failures below — each phrased as the different next action it calls for.

Cancelling ends the add there and then and frees the row; the host's question simply expires
unanswered.

## The answer is encrypted end to end

The common room is, by the multi-host [trust model](./projects-screen-multi-host.md#trust-model), "a
trusted peer group, **not a cryptographically authenticated one**" — any participant that can join it
appears as an eligible daemon and can receive forwarded calls. A passphrase in plaintext there is
readable by every participant.

So the host publishes an **RSA public key with its question**, and the browser encrypts the answer
under it. The plaintext exists in exactly two places: the operator's own dialog, and the addressed
host's process for the duration of the unlock.

### The fingerprint shown is derived from the key, not read off the wire

The question carries both the host's key and a fingerprint string describing it. **Only the key
encrypts anything**, so the browser computes the fingerprint from the key bytes itself and displays
and pins *that*. The advertised string is public and non-secret: an active peer can replay the genuine
one beside its own key, and an operator comparing it against the value they verified out of band would
see exactly what they expected while their passphrase went somewhere else. A question whose two halves
describe different keys is **refused outright** — a host describing its own key gets it right, so a
disagreement means something rewrote one of them in flight.

### Key continuity, and the honest limit

A host's key is **pinned on first sight**. A **changed** key blocks the flow and is released only by
an operator who confirms, in two deliberate steps, that they checked the new key with the host itself
— it is SSH's own model, and the same warning an operator already recognises from
`REMOTE HOST IDENTIFICATION HAS CHANGED`. A legitimate rotation is therefore possible; a silent
substitution is not.

Where no conclusion is available at all — no key presented, or a browser that will not store a pin —
the dialog says **so**, and does not block. A missing conclusion is never rendered as a first
sighting: claiming "first time seeing this host, key recorded" when nothing was recorded stays false
on every later sighting too, which would leave an active substitution indistinguishable from ordinary
use permanently. A key whose fingerprint is still being computed blocks silently: "which key is this?"
has no answer yet.

⚠ **This is the design decision most worth arguing with.** Pinning makes an active key substitution
*visible*, not impossible, and gives no protection on a first-ever connection to an already
compromised host. The weaker alternative — accept passive-only protection and disclose it in the
dialog — remains reasonable, and was rejected rather than overlooked.

### It requires a secure origin, and the daemon has no TLS

Browsers withhold the Web Crypto API on an insecure origin, and the daemon normally serves tddy-web
over plain `http://` on a LAN address. So **add-key works only where the page is on a secure origin**,
and elsewhere the dialog blocks with the origin named as the reason.

There is deliberately **no fallback**. The only thing behind that API here is a passphrase being
encrypted, so the only fallback available would send it in the clear — to every peer in the room, which
is the exposure the feature exists to remove. Refusing and saying why is the correct behaviour, not a
gap. Making add-key work over a LAN address needs TLS on the daemon, which is a deployment decision
and not part of this screen. See
[`packages/tddy-web/docs/insecure-origin-constraints.md`](../../../packages/tddy-web/docs/insecure-origin-constraints.md).

## Whose question is it

A question belongs to the operator whose session raised it. It is **shown to nobody else and
answerable by nobody else** — otherwise a second operator sees the first's dialog, including the
private-key path they named, and can spend the one-use question with garbage, denying the real add for
its whole lifetime.

The identity is the **GitHub user**, not the host OS user they map to. Two GitHub users mapped to one
OS user — which the daemon's own configuration can express — would otherwise still see and burn each
other's questions. An answer from the wrong operator is refused exactly as an answer to a question
that never existed is, and it does not consume the real one.

A question **expires** if it goes unanswered, releasing the add, and can be answered **once**.

## Picking a key, and why typing one is still possible

The host offers the keys its own user could load, out of that user's `~/.ssh`, described by **type and
fingerprint** — because two paths can hold the same key, and a path is the thing the operator was
already failing to recall.

**A candidate is a key whose `.pub` file sits beside it.** That single rule does all the filtering the
list needs, and it means **no private key is ever opened to build it**: everything an operator reads
comes from the public half. `known_hosts`, `authorized_keys`, `config` and a stray note have no public
half. A denylist of names to skip was rejected — it misses whatever file a future OpenSSH version drops
into `~/.ssh`, and the failure mode is offering a non-key as a key.

The rule's cost is that a key with **no** `.pub` beside it is invisible to the list, which is why the
free-text path field stays: the listing is a convenience over `~/.ssh`, not the boundary of what may be
added. For the same reason the listing reads only `~/.ssh` while a typed path may sit anywhere inside
the operator's own home — a walk of a home directory is a walk of their documents.

**Both fields speak absolute paths, and `~` is expanded nowhere.** Not in the browser, which does not
know the host's home directory, and not on the host, where the confinement check is valuable precisely
because it is a pure function of what the caller sent. A tilde path is refused in the browser, which is
the only place holding the context to explain why; the host's own refusal names no path, on purpose.

## What the operator is told when it fails

Five distinct outcomes, because each calls for something different:

| Reading | Next action |
|---|---|
| The key was added, with its fingerprint | none — check it against `ssh-add -l` if you like |
| That passphrase did not unlock the key | retype it |
| The question expired before it was answered | start the add again |
| No agent answered on this host | start an agent there |
| That key could not be read on this host | name a different key |

**"Cannot read that key" is one message for several causes**, deliberately: absent, unreadable and
"not an OpenSSH private key" are indistinguishable, and it names no path. Told apart, this becomes a
file-existence probe over the operator's home directory for any authenticated session — and the remedy
is the same in every case.

**"That passphrase did not unlock the key" and "this host could not decrypt your answer" are
deliberately identical**, for a sharper reason: told apart they form an adaptive decryption oracle
against the host's long-lived key, one clean bit per attempt, from any authenticated session as often
as it likes. The real cause is written to the host's own log.

## What a key can never do here

- The passphrase is **never** written to disk, logged, or held in daemon state past the unlock, and no
  response is derived from it.
- The key is read **as the operator's own OS user**, from **inside their own home directory** — a
  session mapped to one user cannot name another user's key and have the daemon open it on their
  behalf.
- Removing a key from an agent, and generating one, are not offered.

## What is not reachable yet

⚠ **No screen mounts the row this action lives on.** The add-key control sits in the ssh-agent
section of the tooling row, and nothing in `packages/tddy-web/src` renders that row — its only call
sites are the Cypress specs. Everything above is implemented, covered and reachable over the wire, and
**an operator cannot open a screen that shows it**. Mounting the row belongs to the node that owns it,
and this page describes a contract until then.

## Acceptance criteria

- [x] Starting an add-key flow surfaces a prompt in tddy-web naming the host and the key.
- [x] The answer leaves the browser **encrypted**; the plaintext passphrase never appears in any
      request payload.
- [x] A correct passphrase results in the key being held by that host's agent, visible in the agent's
      key list.
- [x] An incorrect passphrase reports a failure and adds nothing.
- [x] The passphrase is never logged and never written to disk.
- [x] A prompt expires unanswered, releasing the operation.
- [x] A prompt can be answered only once.
- [x] The prompt dialog displays the host's public-key fingerprint.
- [x] A **changed** host key blocks the flow with an explicit warning rather than proceeding.
- [x] When the subscriber goes away, the daemon's prompt-stream task is torn down — no leak.
- [x] Answering rejects an invalid session token and an unknown prompt id.
- [x] An operator **picks** a key from the ones that host reports for their own OS user rather than
      recalling a path, and every path offered is one the add accepts. The listing is built from public
      halves only: no private key is opened to describe a candidate, and a user's absent, empty and
      unreadable `~/.ssh` are one answer.
- [x] The key field never invites a path the host is bound to refuse: both surfaces speak absolute
      paths, and a tilde path is refused in the browser, where the reason can still be stated.
- [x] The fingerprint the dialog displays and pins always belongs to the key that will encrypt the
      answer — including when a second prompt replaces the key while the dialog is open.

## Out of scope

Removing a key from an agent. Generating a key. Expanding `~` on the host — rejected, not deferred,
for the reason above. Persisting a passphrase in any form. TLS on the daemon, which is what would let
this work over a plain LAN address. Changing the daemon's git prompt hardening, which turned out not to
need touching at all.

## Related documentation

- [hosts-screen-tooling.md](./hosts-screen-tooling.md) — the ssh-agent section this action sits in,
  and the read side of the same agent
- [hosts-screen.md](./hosts-screen.md) — the screen and its rows
- [projects-screen-multi-host.md](./projects-screen-multi-host.md) — the common-room trust model this
  feature's crypto is answering
- Daemon: [`packages/tddy-daemon/docs/host-add-key.md`](../../../packages/tddy-daemon/docs/host-add-key.md),
  [`connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md#host-add-key)
- Web: [`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md),
  [`insecure-origin-constraints.md`](../../../packages/tddy-web/docs/insecure-origin-constraints.md)
