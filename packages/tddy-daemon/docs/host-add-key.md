# Host add-key (tddy-daemon)

Loading a private key into a host user's ssh-agent, with the passphrase carried from the browser
**encrypted end to end**. The read side of the agent — what it holds, and whether it answered at all
— is [host-tooling-probe.md § The ssh-agent](host-tooling-probe.md#the-ssh-agent); this is the write
side, and the channel that makes it possible.

The daemon has to **ask a question and wait for the answer**, which is the part with no precedent.
The only server-asks-UI mechanism in tddy is the ACP bidi stream (`AcpService.Session`), and it is
not what this uses: a server stream plus a unary reply mirrors `StreamWorktreeStats` +
`CalculateWorktreeSize`, keeps correlation an explicit `prompt_id` rather than an envelope sequence,
and leaves the reply a unary call that can be transport-restricted the way `mint_local_token` is.

The passphrase reaches the daemon as RSA-OAEP ciphertext, is decrypted in process, unlocks the key,
and is dropped. It is never logged, never written to disk, and never placed in daemon state past the
call — pinned by `the_passphrase_never_appears_in_captured_logs` and
`the_passphrase_is_never_written_to_disk`, which walks the daemon's whole data directory after a
successful add.

## The modules

| Module | Owns |
|---|---|
| `host_prompts.rs` | the registry: issue a prompt, expire it, accept exactly one answer, from exactly one operator |
| `host_keypair.rs` | the host's RSA keypair, its `0600` persistence, the published SPKI DER and its fingerprint, and the decrypt |
| `host_prompt_stream.rs` | the `StreamHostPrompts` pump, and its teardown |
| `host_private_key.rs` | reading the key an operator named — as that operator, out of their own home — and listing the keys they could pick from |
| `ssh_agent_add.rs` | handing an unlocked identity to that user's agent over the wire protocol |
| `connection_service.rs` | the four handlers, their routing and their auth |

## The prompt registry

A prompt is a question one operator's session raised, and it carries four guarantees.

**Ownership.** `PendingPrompt.issued_for` is the operator the question belongs to, and both the feed
and the answer filter on it. Without that field the pump replays every outstanding prompt to every
authenticated subscriber — disclosing the private-key path one operator named to every other
operator watching, and handing each of them a single-use prompt to spend.

**The identity is the resolved GitHub user, not the mapped OS user.** It is the identity all three
prompt RPCs already have: `StreamHostPrompts` and `AnswerHostPrompt` resolve a session to a GitHub
user and stop there, and only `AddHostKey` goes on to map it to an OS user. Keying on the OS user
would also merge two GitHub users that `config.users[]` maps to one — they would still see and burn
each other's prompts, which is the exact thing the field exists to prevent.

**Expiry.** `PROMPT_TTL` is 120 s. An unanswered prompt must not pin an operation forever, and the
operation it pins is an `AddHostKey` call that is still in flight.

**Single use.** The "unanswered" marker *is* the `oneshot::Sender` created in `issue`, so single use
is derived from one field rather than from a flag that can disagree with a stored answer. The
registry keeps **no copy** of the ciphertext: it moves through the channel to the waiter. Parking the
receiver in the record buffers an answer that arrives before the waiter claims it, which a bare
`Notify` would drop.

A mismatched answer is refused **before** the sender is taken, so a prompt that somebody else tried
to answer keeps its one answer — pinned by
`an_answer_from_another_operator_leaves_the_prompt_answerable_by_its_owner`. It is refused with the
same `UnknownPrompt` rejection an id that was never issued gets: telling the two apart would confirm
that some other operator has an add in flight, and name nothing an honest caller needs.

## The host keypair

An answer crossing the LiveKit common room, and possibly a forwarding daemon, is readable by any
participant — [`docs/ft/web/projects-screen-multi-host.md` § Trust
model](../../../docs/ft/web/projects-screen-multi-host.md#trust-model) is explicit that the room is
"a trusted peer group, not a cryptographically authenticated one". Encrypting under the addressed
host's public key removes that exposure.

`PublishedKey` carries the **SPKI DER** (what `SubtleCrypto.importKey("spki", …)` expects) and
`spki_fingerprint`, a `SHA256:<base64-no-pad>` digest over those same bytes. The name is deliberate:
node 5 has a `fingerprint_of` over different bytes, and two same-named functions producing different
digests over "a key" is a collision worth removing.

**2048-bit RSA-OAEP (SHA-256), no hybrid envelope.** A 2048-bit OAEP payload carries ~190 bytes,
comfortably more than any passphrase, at the size every `SubtleCrypto` implementation supports
without argument. An AEAD envelope would be more code for no benefit at this size.

**What it defends against, and what it does not.** It defeats a *passive* relay outright. It does
not, by itself, defeat an *active* peer, because the client learns the public key over the same
unauthenticated channel: a participant advertising itself as `daemon-<instance_id>` can publish its
own key. That is bounded on the client, by key continuity plus a fingerprint an operator can verify
out of band — see [`packages/tddy-web/docs/hosts-screen.md` § Host key
trust](../../tddy-web/docs/hosts-screen.md#host-key-trust). It is the most arguable decision in this
flow, and it is a bound rather than a fix.

**Generation is deferred to the first prompt**, not done at startup: a host nobody adds a key on
never pays for an RSA keygen. The private half is `host-prompt-key.pem` under the keypair's storage
directory, written through **`write_atomic_with_mode`** with `OWNER_ONLY_FILE = 0o600`. That variant
exists because `write_atomic` copies permission bits only from an *existing* target, so a first write
through it creates the swap file at the process umask and publishes a world-readable private key. It
sets the mode at `OpenOptions::mode()` creation time with `create_new(true)`, which makes owner-only
**structural** rather than probabilistic — there is no window in which the key exists at the umask.

Rotation is not automated: deleting the file makes the host generate a fresh keypair on its next
prompt, and every client that had pinned the old one sees a `changed` verdict and must accept the new
key explicitly. That is the intended cost of a rotation, and it is why the accept path exists.

## The prompt pump, and why its teardown is the point

`StreamHostPrompts` is **silent almost all of the time** — a host raises a question only while an
operator is adding a key. A handler learns its subscriber has left by failing to send, and this pump
never attempts a send, so it would park forever on a prompt that will never come: one leaked task
per browser tab that ever opened the screen.

`packages/tddy-codegen/docs/server-streaming.md` names this exact case, and the pump obeys it: the
loop `tokio::select!`s on `tx.closed()` alongside the broadcast receive, so a gone subscriber is
noticed directly rather than inferred from a send. It lives in its own module for the same reason
`livekit_rooms_stream.rs` does — the handler is the auth check and the channel, and the loop that
outlives it belongs where it can be read on its own.

**A leaked pump is unobservable from outside a stream that is silent by design**, so the service
publishes `pending_prompt_pump_count()` for the test to see. The counter increments *synchronously in
the handler, before the spawn*, and decrements in `Drop`;
`stream_host_prompts_rpc.rs::stops_the_prompt_pump_once_the_subscriber_is_gone` asserts `== 1` while
subscribed and then `== 0`, because asserting only the zero would hold equally for an
implementation that never counts. `keeps_an_idle_subscription_open_rather_than_completing_it`
brackets the opposite failure: a completed stream reads to the browser as the daemon dropping the
feed.

A subscriber that arrives after the prompt was raised is still shown it — the pump replays what is
outstanding for that operator — so a client cannot miss the question its own call raised.

## Reading the key an operator named

`AddHostKeyRequest.subject` is free text from a browser, and it is read **as the mapped OS user**,
through `spawner::run_capture_as_user`. A child process rather than `seteuid`, because the daemon is
multi-threaded. Read as the daemon it would let a session mapped to `alice` name
`/home/bob/.ssh/id_rsa` and have the daemon open it on her behalf — and it would diverge from the
rest of the host surface, where node 4's probes and node 5's socket resolution already run per user.

Two guards, and both are needed:

- **Confinement** — `confined_to_home` requires an absolute path, free of `..`, inside the mapped
  user's home. Purely lexical.
- **Privilege** — the bytes are read with that user's own credentials. This is the guard that
  actually holds.

**There is deliberately no `canonicalize`.** Canonicalizing would have to `stat` a path the caller
chose, which restores the existence oracle the single refusal below exists to close. It is not needed
either: a symlink inside `alice`'s home pointing at `bob`'s key resolves *for the kernel* as `alice`,
who cannot read it. The lexical check is about intent and blast radius; the privilege drop is the
boundary.

**Nothing expands `~`, anywhere.** Not in the browser, which does not know the host's home, and not
here — the confinement's value is being a pure function of the caller's own input and their own
account, and an expansion step would put a filesystem lookup back in front of it. Both key fields in
the UI therefore speak absolute paths, and a `~` path is refused in the browser, where the reason can
still be stated.

**One refusal.** Every way a read can fail collapses into `KEY_UNREADABLE`, which names no path.
"No such file" and "not an OpenSSH private key", told apart and returned verbatim, make this endpoint
a file-existence oracle for any path inside the caller's home — and the operator's remedy is the same
in both cases: name a key that is there. `KEY_OUTSIDE_HOME` stays a *distinct* message because it is
decided entirely by the path the caller sent and the account they are mapped to, so it discloses
nothing about the host, and it is the one refusal an operator can act on.

## `ListHostKeyCandidates` — a candidate is a key with a `.pub` beside it

`list_key_candidates` offers the private keys a session's own OS user could load, out of their own
`~/.ssh`, ordered by path. Each `KeyCandidate` carries the absolute `path`, the `key_type` and the
`fingerprint` — **all three derived from `<path>.pub`**, so **no private key is ever opened to build
the list**. A list of keys is not a use of them.

That one rule does all of the filtering the surface needs. `known_hosts`, `authorized_keys`, `config`
and a stray note have no public half; a `.pub` file is not itself a candidate; a directory named like
a key is not a file. A **denylist** of names to skip was rejected: it misses whatever file type a
future OpenSSH version drops into `~/.ssh`, and the failure mode is offering a non-key as a key.

**The rule's cost is why the free-text path field survives the picker.** A key with no `.pub` beside
it is invisible to a listing that never opens private keys, and it is still perfectly loadable — so
the listing is a convenience over `~/.ssh`, not the boundary of what may be added. For the same
reason the listing reads **only** `~/.ssh` while confinement permits a key anywhere under the home: a
walk of a home directory is a walk of the operator's documents.

**It returns a list, never a failure.** A user with no `~/.ssh`, one whose `~/.ssh` this host cannot
read, and one with an empty `~/.ssh` are indistinguishable to the caller — the same collapse
`KEY_UNREADABLE` performs, so an authenticated session cannot use the listing as a probe for what is
on the host.

`offers_only_paths_that_an_add_of_the_same_key_accepts` is the test that keeps the two halves
honest: what the listing offers, the add accepts.

## The add itself

`ssh_agent_add.rs` is separate from node 5's `ssh_agent.rs` because it is the write side, and it
exists as a trait for one reason: the add is the only step that touches the operator's real agent, so
it is the only step a test replaces. Everything before it — the prompt, the encryption, the decrypt,
the unlock — runs for real in the handler tests.

`WireProtocolAgentKeyAdder` speaks the agent protocol, which is what lets the passphrase stay in this
process: `ssh-add` would want a TTY or `SSH_ASKPASS`, and the daemon deliberately has neither. The
socket comes from node 5's `WellKnownAgentSockets` — **called, not reimplemented**, so an operator can
never be shown one agent's key list and have a key added to another — and inherits its limits,
including that on a supervised host the daemon's service account may resolve a socket it is not
allowed to open. That is reported as the refusal it is, never as a missing agent. `ADD_TIMEOUT` is
node 5's `AGENT_TIMEOUT`: an add is a longer message but the same single round trip, so it is not
given longer.

The identity arrives **already unlocked**. No implementation of `SshAgentKeyAdder` ever sees a
passphrase.

## Outcomes, and the two that are deliberately identical

`AddHostKeyResponse` carries an `AddHostKeyOutcome` rather than a bare `added`, because a wrong
passphrase is worth retyping, an absent agent is not, and an expired prompt means answering faster —
three different next actions.

**"Cannot decrypt" and "wrong passphrase" are byte-identical, on purpose.** Distinguishable, they
form an **adaptive RSA-OAEP decryption oracle**: one clean bit per query against the host's
long-lived private key, from any authenticated session, as often as it likes. Both arms now share one
constant so they cannot drift apart, and the real cause goes to the host's log only. `decrypt_blinded`
covers the timing channel of the same attack; collapsing the response covers the explicit one.
`an_answer_this_host_cannot_decrypt_is_refused_exactly_as_a_wrong_passphrase_is` pins the collapse,
and `an_answer_this_host_cannot_decrypt_reports_the_wrong_passphrase_arm` pins that the shared arm is
a *stated* one rather than two responses that happen to agree on `UNSPECIFIED`.

Two outcome gaps are recorded rather than closed: an answer this host cannot decrypt at all, and an
agent that answered and refused, both map to `UNSPECIFIED` with a plain `failure_reason`. Reporting
`WRONG_PASSPHRASE` for the first would send the operator to retype something that will keep failing,
and `NO_AGENT` for the second would be false. Adding arms is a proto change plus a TS regeneration.

An **unencrypted** key at `subject` is added as-is rather than reported as a wrong passphrase, since
`PrivateKey::decrypt` refuses an already-decrypted key and the naive mapping would lie. It still
raises a prompt first.

## Dependencies

`rsa` is pinned at **`=0.9.10`** — a cryptographic implementation, where a patch release changing key
generation or padding behaviour is not something to pick up silently. `ssh-key` gains the
**`encryption`** and **`ed25519`** features: without `encryption`, `PrivateKey::decrypt` does not
exist and the passphrase would have to reach `ssh-add` as a subprocess, which is the design node 5
rejected.

Both were **explicitly approved by the developer** under CLAUDE.md § ASK, with the risk disclosed:
`rsa` 0.9.10 is subject to **RUSTSEC-2023-0071** (Marvin, a timing sidechannel in RSA decryption),
which has **no fixed release upstream**. The mitigations are recorded with the approval:
`decrypt_blinded` for the timing channel, and the collapsed response above for the explicit one.

## Testing

| Level | Where | Covers |
|---|---|---|
| Unit | `host_prompts.rs` | expiry, single use, unknown id, ownership |
| Unit | `host_keypair.rs` | fingerprint stability, decrypt of an independently produced ciphertext, refusal of one addressed elsewhere |
| Unit | `host_private_key.rs` | the confinement, the single refusal, and 13 listing behaviours |
| Integration | `connection_service.rs` (`host_add_key_handler_tests`) | the whole flow — real prompt, real RSA-OAEP, real OpenSSH key unlock — with only the agent stood in for |
| Wire-level | `tests/stream_host_prompts_rpc.rs` | the pump's teardown and its idle liveness |

**Only the agent is a double.** A test that handed the handler a plaintext passphrase and called it
encrypted would prove nothing about the path this exists to build, so `encrypted_for` goes through
the **`rsa` crate's public API directly** rather than through anything in this workspace: the test
pins the *format* `SubtleCrypto` produces — SPKI DER in, RSA-OAEP(SHA-256) out — instead of proving
that we agree with ourselves. Fixture keys are generated per run, because key material checked into a
repository is key material in every clone of it.

**RSA keygen was a real flake source, and the obvious fix was not the fix.**
`[profile.test.package.rsa] opt-level = 3` measured as a no-op (20.13 s → 20.53 s): the prime search
lives in **`num-bigint-dig`**, and overriding *that* gives 20.13 s → 1.08 s, taking the add-key
handler tests from 81.86 s to 13.63 s and a ~3.3 s keygen inside a 10 s timeout down to ~0.76 s.

The log recorder in `host_add_key_handler_tests` is an RAII guard that clears its slot on drop and
fails loudly if a second recording displaces it — an unfiltered recorder is process-wide, and one
left installed collects every other test in the binary into a leaked buffer behind a global mutex.
Its assertion first proves a marker line *was* recorded, so a recorder that failed to install cannot
make "the passphrase is absent" pass by capturing nothing. The disk assertion is guarded the same
way: it fails if the walk found no files to inspect.

## Known limits

**`classify_peer_route` compares the routing instance id while `list_known_hosts` publishes the
durable one.** They are equal under the default `daemon_instance_id_append_startup_timestamp: false`.
With that flag on, a browser sending the durable id gets `invalid_argument` rather than being served
locally. Serving it locally would be the wrong answer anyway — it would show that browser this
daemon's own keys under the addressed host's name — so the refusal is the safer of the two, but it is
a refusal where an operator expects an answer. The divergence belongs to the field itself, shared
with `get_host_tooling` and ~20 other RPCs; closing it means changing `classify_peer_route`.

**The passphrase buffer is a plain `Vec<u8>`** — dropped, not zeroized. `zeroize` is not a
`tddy-daemon` dependency, and adding one without consent is not this flow's call to make.

**Add-key requires a secure browser origin**, which the daemon cannot currently provide over a LAN
address: it has no TLS at all. See
[`packages/tddy-web/docs/insecure-origin-constraints.md`](../../tddy-web/docs/insecure-origin-constraints.md).

## See also

- [connection-service.md § Host add-key](connection-service.md#host-add-key) — the four RPCs, their
  routing and their auth
- [host-tooling-probe.md § The ssh-agent](host-tooling-probe.md#the-ssh-agent) — the read side, and
  socket resolution
- [`packages/tddy-web/docs/hosts-screen.md`](../../tddy-web/docs/hosts-screen.md) — the dialog, the
  pin and the picker
- [`docs/ft/web/hosts-screen-add-key.md`](../../../docs/ft/web/hosts-screen-add-key.md) — the feature
