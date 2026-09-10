# Host and worktree services

**Product area:** daemon
**Status:** Active
**Updated:** 2026-09-09

## Summary

A daemon serves several gRPC services, not one. `connection.ConnectionService` is the largest, but
the machines a daemon knows about and the git worktrees it holds are answered by two services of
their own:

| Service | Methods | Served from |
|---|---|---|
| `host.HostService` | `ListEligibleDaemons`, `ListKnownHosts`, `GetHostTooling`, `StreamHostPrompts`, `AnswerHostPrompt`, `AddHostKey`, `ListHostKeyCandidates`, `StreamHostStats` | [`packages/tddy-host-service`](../../../packages/tddy-host-service/docs/host-service.md) |
| `worktree.WorktreeService` | `ListWorktreesForProject`, `RemoveWorktree`, `StreamWorktreeStats`, `CalculateWorktreeSize`, `CleanWorktree`, `RestoreSessionWorktree`, `ListWorktreeDirectory`, `ReadWorktreeFile`, `StreamReadWorktreeFile` | [`packages/tddy-worktree-service`](../../../packages/tddy-worktree-service/docs/worktree-service.md) |
| `connection.ConnectionService` | the other 73 | [`packages/tddy-daemon`](../../../packages/tddy-daemon/docs/connection-service.md) |

## What a client sees

**The coordinates are the only thing that changed.** No behaviour differs: the same handler code
answers the same request and returns the same response, at a new service name.

**Every transport carries all three.** A new service is a `ServiceEntry` registered beside the
others, so it reaches clients over Connect-HTTP `/rpc`, over the LiveKit common room and over the
local UDS socket — where all three are mounted by one `Server::builder()`, so a caller that reached
`GetHostTooling` on that socket before still does. The RPC Playground lists them without any change,
because it discovers services through gRPC ServerReflection.

**A web bundle and a daemon must be from the same side of the split.** 17 method coordinates moved,
so an older bundle calling a newer daemon gets `unimplemented` on those methods. This is the repo's
standing policy — break freely, migrate every consumer in the same change — and it is stated here
because it is the one user-visible consequence.

## Why the daemon is split at all

`tddy-daemon` was 82,546 source lines in 106 flat modules, the largest package in the workspace,
while its **wiring layer — the part that should be all that remains — was already only 2,715 lines**.
The target state is therefore reached by removing rather than by writing.

`ConnectionService` was one gRPC service with 90 methods and a 111-member Rust trait, implemented by
a single 60-field object. `docs/dev/todo/` recorded the problem three times without anyone acting,
for a reason each entry gave: a split would bury a reviewable feature under a mechanical move. Two
changes made it affordable — an intra-package split that turned the 23,099-line file into a
2,416-line facade over 60 modules, and a `move_module_to_crate` operation in
[`tddy-tools restructure`](../coder/rust-code-restructuring.md) that moves a module across a crate
boundary, re-points its callers from a real `textDocument/references` result and edits both
manifests.

**Two structural facts decide where a seam can go.** First, `pub(crate)` does not cross a crate
boundary, so a subsystem's state must move behind an owned struct or a trait rather than be widened
— which the daemon's 30 `pub trait` ports and 21 trait-injecting builders already made affordable.
Second, Rust crates cannot be mutually dependent, so a module cycle that spans two destination crates
must be cut before either can move; a cycle whose members land in the *same* destination crate needs
no edit at all.

## Where a subsystem's seam is drawn

- **By the service the methods belong to, not by the subsystem's name.** The host-key path
  (`host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add`) lives with the host service
  rather than with auth, because `AddHostKey` and `ListHostKeyCandidates` are host-service methods.
- **The shared surface is a crate, not a wider visibility** —
  [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md).
- **A dependency the moved code cannot carry becomes a port.** A branch belongs to a worktree, but
  what claims one is a session; so `branch_owner` reads sessions through a `SessionListing` port and
  what crosses it is the four fields the ownership rule judges on, rather than everything a session
  is.
- **A proto is cut only where the messages allow it.** The host methods' message closure is 31 and
  the worktree methods' is 20, with zero overlap between them and zero with what stays — so neither
  new proto imports anything, and no shared types file was created for a later node's benefit.

## Related documentation

- [`packages/tddy-host-service/docs/host-service.md`](../../../packages/tddy-host-service/docs/host-service.md)
- [`packages/tddy-worktree-service/docs/worktree-service.md`](../../../packages/tddy-worktree-service/docs/worktree-service.md)
- [`packages/tddy-daemon-kernel/docs/daemon-kernel.md`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md)
- [`packages/tddy-daemon/docs/connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md)
- [Rust code restructuring](../coder/rust-code-restructuring.md) — the operation that performs the moves
- [RPC Playground](rpc-playground.md) — how a client discovers what a participant serves
