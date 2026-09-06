# 2026-08-30 — A guest-side RPC driver for VM-backed daemon acceptance

**Category:** Future enhancement
**Source:** workspace-tool-sandbox changeset, 2026-08-30

`vm_workspace_tool_sandbox_acceptance.rs` proves what a Linux workspace jail *needs* from a real
host — the runner is built, the runner is installed, the delegated subtree admits a per-session
scope — but not the confinement itself end to end, the way the macOS suite does in-process. The
blocker is that nothing in the guest can drive the installed daemon: `MintLocalToken` is a tonic
gRPC call over the UDS (peer credentials), `tddy-tools remote` targets a *relay* rather than a
plain daemon, and no `grpcurl` is installed. The daemon's `/rpc` does accept
`application/json` (`tddy-connectrpc`'s `ConnectUnaryJson`), so a small `tddy-tools` subcommand —
mint a local token, then `StartSession` / `ExecuteTool` and print the response — would unlock
genuine end-to-end acceptance for *every* VM-backed daemon feature, not just this one. Worth doing
once, separately from any feature that wants it.
