# 2026-09-24 — GitHub tokens are sealed in a per-user encrypted credential vault

- **A login's GitHub token is encrypted at rest.** It is sealed into the operator's own credential
  vault in `auth_storage`, under a key their **vault passphrase** derives; the daemon keeps no key,
  so a disk or a backup is ciphertext. See
  [session-auth.md § GitHub access-token retention](../session-auth.md#github-access-token-retention-added-2026-07-26).
- **Signing in always completes, and says where the vault stands** — `OPEN`, `LOCKED`,
  `UNINITIALIZED` or `NONE`. The dashboard asks for the passphrase (or a new one) while the vault is
  closed; "Not now" leaves the rest of the app usable.
- **A restart need not ask again.** A browser that has unlocked the vault holds an unlock key, and
  its next session refresh reopens the vault.
- **A forgotten passphrase is a reset**: the old vault is set aside, never deleted, and credentials
  must be linked again. Only a fresh GitHub sign-in may choose a passphrase.
- **New setting: `github.pending_login_ttl_seconds`** (default 600; `0` never; at most seven days) —
  how long a sign-in's token waits in memory for its vault
  ([§ Security / configuration](../session-auth.md#security--configuration)).
- **PR status says to unlock the credential vault** while it is closed, rather than reading as "no
  PR" ([pr-stack-live-status.md](../../coder/pr-stack-live-status.md#authenticated-pr-status-added-2026-07-26)).
- **Breaking: `github-tokens.json` is not migrated.** The first real login after upgrading asks the
  operator to choose a vault passphrase; the old file can be deleted by hand.
- **Known trade-offs**: an unlock key or the passphrase, plus a copy of the disk, is the plaintext,
  and both cross the plain-http LAN origin; a lineage that stops refreshing without logging out
  keeps the vault open until the daemon exits.

`#keyring` 3/9, [#510](https://github.com/uppin/tddy-coder/pull/510).
