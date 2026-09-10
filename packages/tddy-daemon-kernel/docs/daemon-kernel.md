# Daemon kernel (tddy-daemon-kernel)

The symbols every `tddy-daemon` subsystem reaches into `connection_service` for, in a crate a
subsystem can depend on without depending on the daemon.

The crate's own rustdoc (`src/lib.rs`) is the reference: what is here, who reached for it, and the
de-duplication each symbol went through. This page is the **admission rule** — what belongs in the
kernel and what does not — because that is the question every later `#unbundle` node asks.

## Why a crate and not a wider visibility

**`pub(crate)` does not cross a crate boundary.** A module split is served by widening a field; a
crate split is not served by it at all. Once the consumer is a different crate the shared surface has
to be a crate of its own, which is what makes a subsystem move mechanical rather than impossible.

## What belongs here

| Admitted | Rule |
|---|---|
| A **symbol** several subsystems reach for | the default. The unit is the transitive call closure, measured before the move — `spawn_as_user` is 179 of `spawner.rs`'s 2,539 lines; `privilege_drop` 63 of 400; `user_paths` 34 of 210 |
| A symbol that exists **more than once** | consolidating it is a behaviour decision made once, here, and stated. `now_unix_ms` saturates because the alternatives were an `i64` refusal and a bare `as u64` cast that truncates |

Every origin module re-exports every lifted name, so no caller in `tddy-daemon` changes and there
stays exactly one definition of each.

## What does not

- **A subsystem.** The model registry, telegram, screen sharing, sandbox, spawn, auth and LiveKit are
  services in their own right and belong to their own crates.
- **A module, as a rule.** `config.rs` is the single exception, and a deliberate one: four moving
  modules and every handler in both new services take `&DaemonConfig` and read disjoint parts of it,
  so there is no smaller cut — the symbol *is* the file. The alternative, a narrow value struct per
  consuming crate, is authoring rather than moving, and every later node would repeat it.
- **Anything the moving families do not reach.** `pty_registry.rs` was moved here and then retracted
  untouched on exactly that test.

## See also

- [`packages/tddy-daemon/docs/connection-service.md`](../../tddy-daemon/docs/connection-service.md)
- [`docs/ft/daemon/host-worktree-services.md`](../../../docs/ft/daemon/host-worktree-services.md)
- [changesets/](./changesets/)
