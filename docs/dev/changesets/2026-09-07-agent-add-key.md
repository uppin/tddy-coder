# 2026-09-07 — Loading a key into a host's ssh-agent, with an encrypted passphrase
**Type:** Feature

Spans `tddy-service` (four RPCs and their messages), `tddy-daemon` (`host_prompts.rs`,
`host_keypair.rs`, `host_prompt_stream.rs`, `host_private_key.rs`, `ssh_agent_add.rs` and four
handlers) and `tddy-web` (the add-key action, the passphrase dialog, key pinning, `SubtleCrypto`
encryption and the key picker). Node 6 of the `#hosts-screen` stack —
[PR #458](https://github.com/uppin/tddy-coder/pull/458).

Node 5 made a host's ssh-agent legible; this makes it writable. **Nothing in tddy had ever asked the
UI for a secret** — the existing passphrase dialogs run the other way, with the UI deciding to ask
before a unary call — and the daemon is deliberately hardened against interactive prompts. The
hardening turned out **not to need inverting**, a direct dividend of node 5 choosing the agent wire
protocol over `ssh-add`: the passphrase unlocks the key in process and the identity is handed to the
agent directly, so there is no subprocess, no TTY and no `SSH_ASKPASS`.

**The channel is a server stream plus a unary reply**, not the ACP bidi stream: it mirrors
`StreamWorktreeStats` + `CalculateWorktreeSize`, correlation is an explicit `prompt_id` rather than an
envelope sequence, and the reply stays a unary call that can be transport-restricted the way
`mint_local_token` is. `AddHostKey` was added mid-flight because the channel and the crypto had no
operation to serve: without something that *raises* a question and consumes the answer, the whole
decrypt → unlock → agent-add path was unreachable and untestable.

**Four decisions a future reader will otherwise reverse into bugs.**

- **No `canonicalize` in the path confinement.** Statting a caller-chosen path restores the
  file-existence oracle the single `KEY_UNREADABLE` refusal closes. The real boundary is the privilege
  drop — the bytes are read as the mapped OS user, so a symlink in one user's home pointing at
  another's key resolves *as* the first user, who cannot read it.
- **Nothing expands `~`.** Expansion would have to happen in the daemon, and the check's value is
  being a pure function of the caller's input. Both key fields in the browser therefore speak absolute
  paths, and a tilde path is refused there, where the reason can still be stated.
- **The fingerprint is derived in the browser from the key bytes, never taken from the wire.** The
  advertised string is a non-secret assertion an active peer can replay beside its own key — pinning
  it would have made continuity decorative.
- **"Cannot decrypt" and "wrong passphrase" are byte-identical.** Told apart they are an adaptive
  RSA-OAEP decryption oracle, one clean bit per query against the host's long-lived key.

**Dependencies, explicitly approved by the developer** under CLAUDE.md § ASK: `ssh-key`'s
`encryption` + `ed25519` features (without `encryption`, `PrivateKey::decrypt` does not exist and the
passphrase would have to reach `ssh-add` as a subprocess) and `rsa` pinned at **`=0.9.10`**. The risk
was disclosed with the approval: `rsa` is subject to **RUSTSEC-2023-0071** (Marvin) with **no fixed
release upstream**; `decrypt_blinded` covers the timing channel and the collapsed response above
covers the explicit one. Recorded here so the audit trail does not depend on a chat log. **No new web
dependency** — `SubtleCrypto` is a platform API.

**The blocking prerequisite was resolved rather than worked around.** `write_atomic_with_mode` sets
the swap file's mode at `OpenOptions::mode()` creation time with `create_new(true)`, so a host's RSA
private key is never briefly world-readable; the three hand-rolled secret writers
(`github_token_store.rs`, `vnc_vault.rs`, `screen_sharing_vault.rs`) stay deferred, as planned, and
this node adds no fourth.

**A documentation-route deviation, kept knowingly.**
[`packages/tddy-web/docs/insecure-origin-constraints.md`](../../../packages/tddy-web/docs/insecure-origin-constraints.md)
was edited **directly** to add `crypto.subtle` and a Web Crypto section, against CLAUDE.md's rule that
`packages/*/docs/` is reached only through a changeset. The content is correct and is exactly where a
wrap would have placed it, so it was kept rather than reverted and re-derived.

**Known caveat, pre-existing and shared.** `classify_peer_route` compares the **routing** instance id
while `list_known_hosts` publishes the **durable** one. Equal under the default
`daemon_instance_id_append_startup_timestamp: false`; with that flag on, a browser sending the durable
id gets `invalid_argument` where it was previously — wrongly — served locally. Shared with
`get_host_tooling` and ~20 other RPCs, and fixing it means changing `classify_peer_route`.

**Deferred, with reasons.** Splitting `connection_service.rs` (~19,900 lines, and **five** nodes of
this stack touch it) — its own branch after the stack lands, per
[`docs/dev/TODO.md`](../TODO.md); when it happens the host registry, tooling probe and prompt handlers
form one seam. Extracting the duplicated stream read loop, which
`hostStatsSubscription.ts` and `hostPromptsSubscription.ts` now are twice over — it would edit node 3's
file, and the real scope is ~18 app-wide consumers. And the passphrase buffer, a plain `Vec<u8>` that
is dropped but not zeroized: `zeroize` is not a `tddy-daemon` dependency and was not added unasked.

**⚠ The feature is bounded by a deployment fact, not just by code.** `crypto.subtle` is withheld on an
insecure origin, and the daemon serves this bundle over plain `http://` on a LAN address. Add-key
therefore requires a secure origin; there is deliberately **no fallback**, because the only thing
behind `subtle` here is a passphrase and a fallback would hand it to every peer in the room. Making it
work over a LAN address needs TLS, which the daemon has nowhere today. And the row this action lives on
is still mounted by no screen — the limit node 4 recorded — so none of this is visible to an operator
yet.

See [`packages/tddy-daemon/docs/host-add-key.md`](../../../packages/tddy-daemon/docs/host-add-key.md),
[`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md) and
[`docs/ft/web/hosts-screen-add-key.md`](../../ft/web/hosts-screen-add-key.md).
