# 2026-09-07 — Four `ConnectionService` RPCs for loading a key into a host's ssh-agent
**Type:** Feature

`connection.proto` gains the surface behind
[`docs/ft/web/hosts-screen-add-key.md`](../../../../docs/ft/web/hosts-screen-add-key.md): a
server-stream for the question a host is waiting on, a unary reply carrying the **encrypted** answer,
the operation that raises one and consumes it, and a listing so the key can be picked rather than
recalled.

```proto
rpc StreamHostPrompts(StreamHostPromptsRequest) returns (stream HostPromptEvent);
rpc AnswerHostPrompt(AnswerHostPromptRequest) returns (AnswerHostPromptResponse);
rpc AddHostKey(AddHostKeyRequest) returns (AddHostKeyResponse);
rpc ListHostKeyCandidates(ListHostKeyCandidatesRequest) returns (ListHostKeyCandidatesResponse);
```

| Message | Fields |
|---|---|
| `StreamHostPromptsRequest` | `session_token` 1, `daemon_instance_id` 2 |
| `HostPromptEvent` | `prompt_id` 1, `daemon_instance_id` 2, `kind` 3 (`HostPromptKind`), `subject` 4, `bytes host_public_key` 5, `host_public_key_fingerprint` 6, `int64 expires_at_unix_ms` 7 |
| `AnswerHostPromptRequest` | `session_token` 1, `daemon_instance_id` 2, `prompt_id` 3, `bytes encrypted_answer` 4 |
| `AnswerHostPromptResponse` | `accepted` 1, `rejection_reason` 2 |
| `AddHostKeyRequest` | `session_token` 1, `daemon_instance_id` 2, `subject` 3 |
| `AddHostKeyResponse` | `added` 1, `outcome` 2 (`AddHostKeyOutcome`), `fingerprint` 3, `failure_reason` 4 |
| `ListHostKeyCandidatesRequest` | `session_token` 1, `daemon_instance_id` 2 |
| `HostKeyCandidate` | `path` 1, `key_type` 2, `fingerprint` 3 |
| `ListHostKeyCandidatesResponse` | `repeated HostKeyCandidate candidates` 1 |

Enums: `HostPromptKind` (`UNSPECIFIED` 0, `SSH_KEY_PASSPHRASE` 1) and `AddHostKeyOutcome`
(`UNSPECIFIED` 0, `ADDED` 1, `WRONG_PASSPHRASE` 2, `PROMPT_EXPIRED` 3, `NO_AGENT` 4,
`KEY_UNREADABLE` 5).

**A server stream plus a unary reply, not the ACP bidi stream.** It mirrors `StreamWorktreeStats` +
`CalculateWorktreeSize`, correlation is an explicit `prompt_id` rather than an envelope sequence, and
the reply stays a unary call that can be transport-restricted the way `mint_local_token` is.

**`host_public_key` is the SPKI DER**, which is what `SubtleCrypto.importKey("spki", …)` expects, and
`encrypted_answer` is the RSA-OAEP(SHA-256) result over it. `host_public_key_fingerprint` is an
**assertion, not a credential**: it is public, an active peer can replay it beside its own key, and the
client therefore derives the fingerprint it displays and pins from `host_public_key` itself. A frame
whose two halves describe different keys is refused by the client.

**`subject` is an absolute path on the host**, and nothing anywhere expands `~` — the daemon's
confinement check is deliberately a pure function of the caller's input, so a tilde path is refused in
the browser rather than sent to a refusal that names no path. The proto comment says so, because it
previously described "a path on the host" and stopped there while the handler rejected anything
relative.

**`AddHostKeyResponse` carries an enum rather than only `added`** so the browser can tell a wrong
passphrase (retype it) from an expired prompt (be quicker), an absent agent (start one) and an
unreadable key (name another). Two failures have no arm of their own on purpose: an answer this host
cannot decrypt shares `WRONG_PASSPHRASE`'s message byte for byte, because telling them apart is an
adaptive RSA-OAEP decryption oracle; and an agent that answered and refused falls to `UNSPECIFIED`
with a plain `failure_reason`, since `NO_AGENT` would be false.

**`daemon_instance_id` means the same thing on all four as on `GetHostTooling`:** empty is the daemon
serving the call, and a request addressed elsewhere is routed rather than answered locally.

Node 6 of the `#hosts-screen` stack — [PR #458](https://github.com/uppin/tddy-coder/pull/458).

See [`packages/tddy-daemon/docs/connection-service.md § Host
add-key`](../../../tddy-daemon/docs/connection-service.md#host-add-key) and
[`host-add-key.md`](../../../tddy-daemon/docs/host-add-key.md).
