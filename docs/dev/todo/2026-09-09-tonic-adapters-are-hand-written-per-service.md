# 2026-09-09 — The local socket's tonic adapters are hand-written, one `async fn` per method

**Category:** Future enhancement
**Source:** `#unbundle` node 1, [#470](https://github.com/uppin/tddy-coder/pull/470)

`tddy-codegen`'s `generate_tonic_adapter` is a stub, so every service the daemon serves on the local
UDS socket needs an adapter written by hand: `connection_tonic_adapter.rs` already, and #470 added
`host_tonic_adapter.rs` (8 methods) and `worktree_tonic_adapter.rs` (9). Each is a literal
`async fn` per method that unwraps a `tonic::Request`, calls the Connect-RPC impl and maps the result
back.

**A macro cannot stand in for the generator.** `#[tonic::async_trait]` rewrites the signatures of the
trait it is applied to, and a declarative macro cannot see through that rewrite to generate the
bodies — which is why this is a codegen job and not a five-line `macro_rules!`.

The cost is linear in the `#unbundle` stack: every remaining node that splits a service out of
`connection.ConnectionService` writes another one. Two mitigations are already in place —
`to_tonic_status` is shared, so three adapters cannot drift on how a refusal maps to a tonic code,
and all three services are mounted on the **same** socket by one `Server::builder()`, so no caller's
coordinates changed. Implementing `generate_tonic_adapter` was out of scope for a node whose subject
was the move operation itself.
