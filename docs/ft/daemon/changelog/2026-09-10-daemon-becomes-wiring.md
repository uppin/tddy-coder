# 2026-09-10 — ConnectionService dissolved — daemon is wiring

The `#unbundle` stack finishes: **`connection.ConnectionService` no longer exists.** The last seventeen
methods move to dedicated coordinates; the daemon assembles and serves them but implements only its
own config service and PR-stack RPC surface.

**`session.SessionService`** (eight methods) lives in **`tddy-session-lifecycle`**. **`project.ProjectService`**
(five methods) lives in **`tddy-projects`**. **`demo_vm.DemoVmService`** and **`local_token.LocalTokenService`**
join their existing crates. The local Unix socket carries every registered service, not a single
facade.

## For operators

Upgrade **daemon, web bundle, coder session participant, and any local-socket clients together**.
Any client still calling `connection.ConnectionService` will fail — there is no fallback coordinate.

`MintLocalToken` remains **local-socket only**; moving the signer did not widen transport reach.

Technical reference: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md),
[session-service.md](../../../packages/tddy-session-lifecycle/docs/session-service.md),
[project-service.md](../../../packages/tddy-projects/docs/project-service.md).

Stack context: node 8 changelog [2026-09-12-unbundle-exec-prstack-services.md](./2026-09-12-unbundle-exec-prstack-services.md) described the penultimate split; this release removes the residual service entirely.
