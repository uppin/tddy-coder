# 2026-10-25 — Dev daemons open their credential vaults without a passphrase

- New `github.dev_vault_without_passphrase: true`: the daemon creates and opens every credential
  vault under the empty passphrase itself, at sign-in and at every session refresh, so nothing asks
  for a vault passphrase — including after a restart, where the prompt used to be refused for want
  of a fresh sign-in.
- **Development only.** A daemon whose `github:` block is not `stub: true` does not start with it,
  and one that has it says at startup that its vaults are as good as plaintext on disk.
- A vault already created under a real passphrase is left alone and still asks for it.
- On in `dev.daemon.yaml` (`./web-dev`) and `dev.desktop.yaml` (`./desktop-dev`), which now also
  keeps vaults (`auth_storage: tmp/.tddy/auth`).
