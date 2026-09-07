# 2026-09-07 — Asking an operator for a passphrase, and loading the key it unlocks
**Type:** Feature

Five new modules and four handlers let a host raise a question, wait for an encrypted answer, unlock
an OpenSSH private key with it and hand the identity to an ssh-agent — the write side of the agent
node 5 made legible. Full account in [`host-add-key.md`](../host-add-key.md); the RPC surface is in
[`connection-service.md § Host add-key`](../connection-service.md#host-add-key).

`host_prompts.rs` is the registry. The **"unanswered" marker is the `oneshot::Sender` itself**, so
single use is derived from one field rather than a flag that can disagree with a stored answer, and
the registry keeps **no copy** of the ciphertext — it moves through the channel to the waiting
`AddHostKey`. Parking the receiver in the record buffers an answer that arrives before the waiter
claims it, which a bare `Notify` would drop.

**A prompt is owned, and the owner is the GitHub user.** `issued_for` gates both the feed and the
answer. Not the mapped OS user: `config.users[]` can map two GitHub users to one, and they would then
see and burn each other's prompts — which is the very thing the field exists to prevent. It is also
the identity all three prompt RPCs already have, since only `AddHostKey` goes on to resolve an OS
user. A cross-operator answer gets the `UnknownPrompt` rejection an unissued id gets, and is refused
**before** the sender is taken, so the prompt keeps its one answer.

`host_keypair.rs` holds the host's 2048-bit RSA keypair, publishes the SPKI DER plus a
`spki_fingerprint` over those same bytes, and decrypts with `decrypt_blinded`. Generation is deferred
to the first prompt, so a host nobody adds a key on never pays for a keygen. The private half is
`host-prompt-key.pem` written through **`write_atomic_with_mode`** at `0600` — the variant exists
because `write_atomic` copies permission bits only from an *existing* target, so a first write through
it publishes a world-readable private key; `OpenOptions::mode()` plus `create_new(true)` makes
owner-only structural rather than probabilistic. `fingerprint_of` was renamed to `spki_fingerprint` to
remove the collision with node 5's same-named function over different bytes.

`host_prompt_stream.rs` is the pump, split out for the reason `livekit_rooms_stream.rs` was. **Its
teardown is the module's whole point.** The feed is silent unless somebody is adding a key, so the
send failure a handler normally learns from is never attempted: the loop `tokio::select!`s on
`tx.closed()`, or it leaks one task per subscription forever. The service publishes
`pending_prompt_pump_count()` because a leak is otherwise unobservable from outside a stream that is
silent by design, and the counter increments **synchronously in the handler before the spawn** — the
regression test asserts `== 1` while subscribed and only then `== 0`, since the zero alone holds for an
implementation that never counts.

`host_private_key.rs` reads the caller-named key **as the mapped OS user**, through
`spawner::run_capture_as_user` — a child process, not `seteuid`, because the daemon is multi-threaded.
Confinement is lexical and **deliberately without `canonicalize`**: canonicalizing would stat a path
the caller chose, restoring the existence oracle the single refusal closes, and it is unnecessary
because the privilege drop is the real boundary — a symlink in `alice`'s home pointing at `bob`'s key
resolves for the kernel as `alice`, who cannot read it. **Nothing expands `~`**, here or anywhere: the
check's value is being a pure function of the caller's input. Absent, unreadable and malformed collapse
into one `KEY_UNREADABLE` that names no path; `KEY_OUTSIDE_HOME` stays distinct because it is decided
by the caller's own input and account and discloses nothing about the host.

Its `list_key_candidates` answers `ListHostKeyCandidates`. **A candidate is a key whose `.pub` sits
beside it** — one rule that does all the filtering (`known_hosts`, `authorized_keys`, `config` and a
stray note have no public half) and that means **no private key is ever opened to build the list**. A
denylist of names was rejected: it misses whatever file a future OpenSSH version adds. The rule's cost
is that a key with no `.pub` is invisible, which is why the browser keeps a typed-path field. It returns
a list and never a failure, so an absent, unreadable and empty `~/.ssh` are one answer and an
authenticated session cannot probe the host with it.

`ssh_agent_add.rs` is the write trait, separate from node 5's read module and the only step of the
flow a test replaces. Socket resolution is **called, not reimplemented**, so an operator can never be
shown one agent's keys and have a key added to another; it inherits that resolver's supervised-host
limit, reported as the refusal it is rather than as a missing agent.

**Two refusals are byte-identical on purpose.** "This host cannot decrypt your answer" and "that
passphrase did not unlock the key" share one constant so they cannot drift; told apart they are an
**adaptive RSA-OAEP decryption oracle**, one clean bit per query against the host's long-lived key,
from any authenticated session. The real cause goes to the log only.

`daemon_instance_id` is honoured on all four handlers, mirroring `get_host_tooling` — the proto had
promised routing no handler implemented, and a key silently loaded into the wrong host's agent is a
worse version of the failure that handler's own comment warns about. The pre-existing routing-id /
durable-id caveat this shares with ~20 other RPCs is recorded in
[`host-add-key.md § Known limits`](../host-add-key.md#known-limits).

**Dependencies, explicitly approved by the developer** under CLAUDE.md § ASK, with the risk disclosed:
`ssh-key`'s **`encryption`** and **`ed25519`** features (without `encryption` there is no
`PrivateKey::decrypt`, and the passphrase would have to reach `ssh-add` as a subprocess) and `rsa`
pinned at **`=0.9.10`**, which is under **RUSTSEC-2023-0071** (Marvin) with **no fixed release
upstream**. `decrypt_blinded` and the collapsed response are the recorded mitigations.

**Testing: only the agent is a double.** `encrypted_for` goes through the `rsa` crate's public API
directly, so the tests pin the *format* `SubtleCrypto` produces rather than proving we agree with
ourselves. Fixture keys are generated per run. RSA keygen was a real flake source and the obvious fix
was wrong — `[profile.test.package.rsa] opt-level = 3` measured as a no-op (20.13 s → 20.53 s);
overriding **`num-bigint-dig`**, where the prime search actually lives, gives 20.13 s → 1.08 s and takes
the handler tests from 81.86 s to 13.63 s.

**Deferred.** Splitting `connection_service.rs` (~19,900 lines; five nodes of this stack touch it) —
its own branch after the stack lands, per [`docs/dev/TODO.md`](../../../../docs/dev/TODO.md). And the
passphrase buffer: a plain `Vec<u8>`, dropped but **not zeroized**, because `zeroize` is not a
dependency of this crate and was not added unasked.

Node 6 of the `#hosts-screen` stack — [PR #458](https://github.com/uppin/tddy-coder/pull/458).

See [`host-add-key.md`](../host-add-key.md), [`connection-service.md`](../connection-service.md) and
[`host-tooling-probe.md`](../host-tooling-probe.md).
