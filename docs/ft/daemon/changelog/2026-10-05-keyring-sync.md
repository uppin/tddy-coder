# 2026-10-05 — Your credentials propagate only to daemons you authorise

- **A daemon shares your credentials with the peers you've authorised, never with everyone in the
  room.** Set `keyring.group_secret` the same way on every daemon that should share your
  credentials; leave it unset and that daemon shares with nobody — the common case for a desktop
  install with no fleet.
- **Each side knows where it stands.** The Accounts screen shows a badge per account — synced,
  pending, undeliverable, in conflict, or refused — the worst standing across every peer it has been
  offered to. No badge means nothing has synced it yet.
- **Removing an account removes it everywhere it had reached**, rather than letting a peer that still
  held the old credential hand it back.

See [Journaled credential propagation](../credential-sync.md).
