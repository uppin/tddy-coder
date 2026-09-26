# 2026-09-25 — `move_test_binary_to_crate` cannot see through the glob facade a module move writes, so the moved test keeps naming the origin

**Category:** Future enhancement (engine defect)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02b-task-action-services-to-daemon-sandbox.jsonl`,
ops 2 and 3 (`tests/action_service_acceptance.rs` and `tests/action_sandbox_acceptance.rs` →
`tddy-daemon-sandbox`)

## What happened

Ops 0 and 1 of the same plan moved `task_service` and `action_service` from
`tddy-session-lifecycle` to `tddy-daemon-sandbox`, each with `"reexport":"glob"`. The facade they
leave in the origin's `lib.rs` is a crate-root glob:

```rust
// packages/tddy-session-lifecycle/src/lib.rs, after ops 0–1
pub use tddy_daemon_sandbox::*;
```

Ops 2 and 3 then moved the two test binaries that exercise those modules. The documented contract
is that every path opening with the origin's extern name is re-pointed at the crate that **defines**
what it reaches. Neither file was changed at all (a pure rename), and the destination gained a
dev-dependency on the crate the code just left:

```rust
// packages/tddy-daemon-sandbox/tests/action_service_acceptance.rs, as the engine left it
use tddy_session_lifecycle::action_service::ActionServiceImpl;
use tddy_session_lifecycle::task_service::TaskServiceImpl;
```

```toml
# appended to packages/tddy-daemon-sandbox/Cargo.toml [dev-dependencies]
tddy-session-lifecycle = { path = "../tddy-session-lifecycle" }
```

That compiles, since cargo permits a dev-dependency cycle. But the receiver now depends on its
origin, and the whole point of the move was to prevent that (`#carve` 15/15 Boundaries: "No
receiver depends on `tddy-session-lifecycle`"). On this plan the compile gate failed on the
[extern-name self-dependency](./2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name.md)
instead, so this defect showed up only in the diff.

## Why

`module_home::defining_module_in_crate` answers "which crate defines `action_service`?" by reading
the origin's `lib.rs`. `re_export_target` matches only a `use` path that **names** the module
(`pub use x::action_service;`, or a group containing it). A glob names nothing, and the `pub mod`
line is gone, so the lookup falls through to `Ok(None)`, meaning "the origin defines it". The facade
that `move_module_to_crate` itself writes is the one shape `defining_crate` cannot follow.

## What was fixed by hand

```rust
// both moved test binaries
use tddy_daemon_sandbox::action_service::ActionServiceImpl;
use tddy_daemon_sandbox::task_service::TaskServiceImpl;
```

and the `tddy-session-lifecycle` dev-dependency removed from `tddy-daemon-sandbox/Cargo.toml`.

## What would fix it

Either would work:
- The re-point consults the plan's own earlier moves, so a module op 0 sent to crate X is defined in X.
- `defining_module_in_crate` treats a crate-root `pub use <crate>::*;` as a candidate and confirms it
  by checking whether `<crate>`'s `lib.rs` declares the module.

## Also seen on this plan (causes already recorded)

- **One facade per operation.** Ops 0 and 1 each appended an identical `pub use tddy_daemon_sandbox::*;`
  to the origin's `lib.rs` ([`2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md`](./2026-09-18-cross-crate-move-cosmetic-facade-and-mod-ordering.md)).
  This is not only cosmetic. rustc reports the second line as `unused import: tddy_daemon_sandbox::*`,
  so `clippy -D warnings` fails. It was deleted by hand.
- **The pre-apply compile gate checks only the packages owning the plan's files.** That means the
  origin, not the destination. `tddy-daemon-sandbox`'s `tests/sandbox_stdio_seatbelt_acceptance.rs`
  (`#![cfg(target_os = "macos")]`) does not compile on HEAD `d9f8f7b3` (`E0425`, `SandboxHandle`
  not imported). The pre-gate did not see it, and the post-apply gate then reported it among the
  plan's errors.
