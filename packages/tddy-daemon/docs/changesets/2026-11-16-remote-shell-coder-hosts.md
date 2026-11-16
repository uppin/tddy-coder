# 2026-11-16 — The daemon lists a caller's Coder accounts from their vault

**Type:** Feature

`VaultCoderAccountSource` fills the host service's `CoderAccountSource` port from the daemon's vaults: a subject's connected Coder accounts with their labels, through the same records `ListAccounts` reads. It has its own tests. The runtime wires the daemon's Coder credential source into the session host, so starts and resumes on a Coder workspace read the account from the caller's vault. See [the cross-package entry](../../../docs/dev/changesets/2026-11-16-remote-shell-coder-hosts.md).
