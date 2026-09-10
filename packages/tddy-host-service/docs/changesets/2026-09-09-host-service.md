# 2026-09-09 — `host.HostService` in its own crate

**Type:** Architecture

New crate, added by the root node of the `#unbundle` stack
([#470](https://github.com/uppin/tddy-coder/pull/470)). Full story in the cross-package entry:
[2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

13 modules left `tddy-daemon` and now serve eight methods as `host.HostService`:
`ListEligibleDaemons`, `ListKnownHosts`, `GetHostTooling`, `StreamHostPrompts`, `AnswerHostPrompt`,
`AddHostKey`, `ListHostKeyCandidates`, `StreamHostStats`. Surface and wiring:
[host-service.md](../host-service.md).

**The host-key path travels with hosts, not with auth.** `AddHostKey` and `ListHostKeyCandidates` are
host-service methods, so `host_keypair`, `host_private_key`, `ssh_agent` and `ssh_agent_add` belong
to the service that serves them — and moving them here cuts `host_tooling ⇄ ssh_agent`, a cycle no
crate boundary could tolerate, as a side effect of a move that was going to happen anyway.

**The proto cut needed no shared types file.** The closure of messages these eight methods reach is
31, with zero overlap with the worktree service's 20 and zero with everything that stayed, so
`host.proto` imports nothing.

**Five of the eight route to a peer before the caller is authenticated** — a host question is
answered by the host it is about. `tddy-daemon` hands this crate the config, roster and token
resolver as `ConnectionServiceImpl::routing_view` for exactly that decision.

The tests travelled with the code: five handler-test modules in `src/` driving `HostServiceImpl`
through the `host.HostService` trait, plus `tests/stream_host_prompts_rpc.rs`. 129 passing.
`host_prompts::answer_before_expiry` widened to `pub` for `screen_sharing_service.rs`, which stayed
in the daemon.

Moved with the code: [host-registry.md](../host-registry.md),
[host-tooling-probe.md](../host-tooling-probe.md), [host-add-key.md](../host-add-key.md).
