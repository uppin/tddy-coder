# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`askpass` module: `AskpassEndpoint`, `CloneGrants` / `CloneGrant`, `AskpassOutcome`, `Passphrase`. A loopback endpoint a clone's helper calls; it raises the existing `SshKeyPassphrase` prompt for the clone's GitHub user and decrypts the answer with the host keypair. `test_util::encrypt_for_host` stands in for the browser. See [clone askpass](../clone-askpass.md).
