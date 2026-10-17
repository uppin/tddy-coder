# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

The runtime starts one `AskpassEndpoint` on the registry and keypair `HostServiceImpl` publishes and installs it on the session host before the RPC handlers. See [clone askpass](../../../tddy-host-service/docs/clone-askpass.md#wiring).
