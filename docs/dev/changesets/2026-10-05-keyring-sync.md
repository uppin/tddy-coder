# 2026-10-05 — Journaled credential propagation between daemons

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)

Propagates `tddy-credentials`' records to the peers authorised to hold them — wrapped per
recipient, gated by two independent checks (a configured `keyring.group_secret` and `#keyring` 1/9's
Ed25519 signature), reconciled last-writer-wins with tombstones, and journaled on both sides so
every daemon knows its sync status. Room membership is explicitly not the gate.

## What changed, by package

- **`tddy-credential-sync`** (new crate): the engine, the two admission checks, the X25519 wrapping,
  last-writer-wins reconciliation and the journal. Detail:
  [`docs/credential-sync.md`](../../packages/tddy-credential-sync/docs/credential-sync.md).
- **`tddy-credentials`**: `CredentialRecord.version`, `Tombstone`, `VaultEntry`; `SessionVault::remove`
  tombstones instead of deleting, `SessionVault::entries` surfaces everything including deletions.
  Detail: [`docs/credential-store.md`](../../packages/tddy-credentials/docs/credential-store.md).
- **`tddy-daemon-livekit`**: `LiveKitPeerTransport`, the `PeerTransport` adapter over the common
  room's participant attributes. Detail:
  [`docs/livekit-service.md`](../../packages/tddy-daemon-livekit/docs/livekit-service.md).
- **`tddy-daemon-kernel`**: `DaemonConfig.keyring: Option<KeyringConfig>` —
  `group_secret`, absent meaning sync with nobody. Detail:
  [`docs/daemon-kernel.md`](../../packages/tddy-daemon-kernel/docs/daemon-kernel.md).
- **`tddy-accounts`**: `AccountSummary.sync_status`, the `SyncStatusSource` port,
  `AccountsServiceImpl::with_sync_status` (optional, additive). Detail:
  [`docs/accounts-service.md`](../../packages/tddy-accounts/docs/accounts-service.md).
- **`tddy-web`**: the Accounts screen's sync-status badge (`SyncStatusBadge`), an aggregate per
  account rather than a per-peer breakdown. Detail:
  [`docs/accounts-screen.md`](../../packages/tddy-web/docs/accounts-screen.md).
- **`tddy-daemon`**: `src/credential_sync.rs` assembles a real `SyncEngine` in `runtime::build` — the
  persisted transport key, the identity verifier bridging `#keyring` 1/9's `KeyDirectory`, the
  signing transport, and the peer-join publish trigger. Detail:
  [`docs/daemon-endpoint.md`](../../packages/tddy-daemon/docs/daemon-endpoint.md).

## Known limitations, carried forward rather than fixed here

- **Exactly one signed-in subject.** The wire format names no subject, so a daemon with more than one
  open vault sits out each sync attempt rather than guess whose record is whose.
  (`docs/dev/todo/2026-10-05-keyring-sync-single-subject-only.md`)
- **A received tombstone cannot yet be retained with its original version/`deleted_at`**, and nothing
  registers the receiving-side RPC handler yet — both named in the same entry above and in
  `tddy-credential-sync`'s own doc, § *Known limitations*.
- **Sync fires on join, not yet on a vault change.**
- **The Accounts screen shows an aggregate, not a per-peer breakdown** — a deliberate scope decision,
  not a gap.
- Two files this PR grew past the 500-production-line budget
  (`tddy-credential-sync/src/engine.rs`, `tddy-credentials/src/vault.rs`) and two functions over
  `/analyze-clean-code`'s thresholds (`SyncEngine::publish`, `credential_sync::build`) — all four
  deferred with the developer's consent rather than restructured on an already-green PR; tracked in
  `docs/dev/todo/2026-10-05-keyring-sync-oversized-files.md` and
  `docs/dev/todo/2026-10-05-keyring-sync-complex-functions.md`.
- Two pre-existing, already massively-oversized files this PR grew further — `tddy-daemon-kernel`'s
  `config.rs` (1,491 → 1,519) and `tddy-daemon`'s `runtime.rs` (1,700 → 1,781 as a file; its `build`
  function 950 → 1,010 by brace matching) — measurements updated in their existing code-issue
  records (`oversized-file-config.md`, `oversized-file-runtime.md`, `complexity-runtime-build.md`),
  consistent with how every prior `#keyring` node deferred the same two files.

## Two defects found and fixed during review

1. `SyncEngine::publish`'s resend-dedup was keyed on a record's `version`, but a tombstone shares its
   record's version by design — so a peer that had already acknowledged a record's current version
   would never receive the tombstone that later deletes it. Re-keyed on `written_at()`, which always
   advances; pinned with a new regression test.
2. A test fixture (not the implementation) hardcoded a `transport_public_key` inconsistent with the
   fixed secret every test actually uses, failing the one test that performs a genuine two-party
   X25519 round trip. Fixed the fixture, not the test.
