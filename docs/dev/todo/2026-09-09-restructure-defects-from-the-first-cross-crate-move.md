# 2026-09-09 — Restructure defects found by the first real cross-crate move

**Category:** Future enhancement
**Source:** `#unbundle` node 1, [#470](https://github.com/uppin/tddy-coder/pull/470) — applying
`move_module_to_crate` to 21 `tddy-daemon` modules on its first live use

The operation moved **13 of the 21**; the other 8 were hand-moved. What stopped it, and what made the
13 more expensive than they should have been. The user-visible limitations are documented in
[`docs/ft/coder/rust-code-restructuring.md` § Known limitations](../../ft/coder/rust-code-restructuring.md#known-limitations);
what follows is the operational debt behind them.

- **The journal is repo-scoped, not plan-scoped** (`.restructure/journal.jsonl`). A *completed* plan
  blocks the next one with *"a journal already exists for this plan — pass `--resume`"*, and
  `--resume` would resume the wrong plan. Every layer of a hand-layered plan needed the journal
  archived by hand first. This is the single largest tax on a multi-layer move.
- **The operation adds the destination as a dependency of itself** when the moved module names a
  sibling that has already moved to the same crate: `tddy-host-service` and `tddy-worktree-service`
  each came out of layer 2 with `tddy-<self> = { path = "" }` in their own manifest, which cargo
  rejects as a cyclic package dependency.
- **A `pub use` facade in the origin makes the dependency-cycle refusal fire spuriously.** After
  `config` moved to `tddy-daemon-kernel`, `tddy-daemon` kept `pub use tddy_daemon_kernel::config;`,
  so rust-analyzer canonicalises `crate::config::DaemonConfig` as `tddy_daemon::config::DaemonConfig`
  and the refusal reads it as an origin dependency the destination would depend back on. Eight moving
  modules had to be re-pointed at `tddy_daemon_kernel::config::` by hand first.
- **Cosmetic**: the operation appends one `pub use <crate>::*;` to the origin's `lib.rs` **per
  operation** rather than one per destination — ten identical lines after a ten-op plan — and appends
  `pub mod` lines after whatever the destination's `lib.rs` already said, rather than in order.
- **The refusals are unit-tested only** (cycle, undeclared module, nested module). Proving a refusal
  against a live server costs a rust-analyzer spawn for a decision made before the server is ever
  consulted, which is why it was not done; it does mean no test asserts the refusal survives a real
  workspace.

## Added by `#unbundle` node 2 ([#471](https://github.com/uppin/tddy-coder/pull/471))

- **A directory-shaped subsystem is entirely out of reach.** `move_module_to_crate` moved **0 of
  13** `model_registry/` modules. `source_crate_of` (`crate_move.rs:773`) requires the anchor file
  to be `<crate>/src/<module>.rs`, so every nested path is refused before rust-analyzer is ever
  spawned:

      $ tddy-tools restructure apply plan.jsonl --dry-run
      Error: plan is malformed: `packages/tddy-daemon/src/model_registry/error.rs` is not
      `<crate>/src/error.rs` — `move_module_to_crate` moves a module the crate root itself declares

  This is documented as a known limitation ("Only `<crate>/src/<module>.rs` moves"), but node 1's
  experience understated its cost: it is not an edge case, it is the *common* shape for a
  subsystem worth extracting. `model_registry/` was picked as node 2's opening move precisely
  because it is the cleanest extraction in `tddy-daemon` — already directory-shaped, zero outbound
  `crate::` edges, zero inline tests — and the operation could not touch a line of it. All 13 were
  hand-moved.

  What would fix it: take the destination's own module path from the anchor's `path` (already
  `model_registry::store`, i.e. the operation is *told* the nesting) and locate the parent's `mod`
  line by walking `<crate>/src/<parent>.rs` then `<crate>/src/<parent>/mod.rs`, rather than
  guessing. Refusing only when neither exists would keep the refusal honest and admit the shape
  that matters.
- **`restructure check` does not catch it.** `check` on the same 13-op plan reported `no findings`;
  the refusal surfaced only under `apply`. A static preflight that cannot tell you the whole plan
  will be rejected is not doing the job `check` exists for.
- **`verify --against` earns its keep.** It found all 91 changed statements repo-wide and made it
  checkable, line by line, that not one of them was moved *logic* — every one was a stub line being
  replaced, a `super::`/`crate::model_registry::` re-point, or the wiring collapsing into the two
  `build_*_entry` calls. Worth saying out loud alongside the defects.

### The facade-cycle refusal, seen a second time (`#unbundle` node 2, M3)

`screen_sharing_service.rs` is exactly the shape `move_module_to_crate` supports —
`packages/tddy-daemon/src/screen_sharing_service.rs`, declared by the crate root, no nesting — so it
got past `source_crate_of`. It was then refused for the *other* reason already recorded above, the
`pub use` facade:

    Error: plan is malformed: `packages/tddy-daemon/src/screen_sharing_service.rs` still names
    `tddy-daemon` (tddy_daemon::config::{resolve_rdp_binary_path, resolve_vnc_binary_path,
    DaemonConfig}, tddy_daemon::host_desktop_targets::{HostDesktopTarget, HostDesktopTargetStore},
    tddy_daemon::host_keypair::HostKeypair, tddy_daemon::host_prompts::{answer_before_expiry,
    HostPromptRegistry, PromptKind}, tddy_daemon::screen_sharing_vault::{), so the destination would
    depend on the crate it left while that crate goes on naming the module it lost

Four of the five paths are node 1's own facades — `pub use tddy_daemon_kernel::config;` and
`pub use tddy_host_service::{host_desktop_targets, host_keypair, host_prompts, …}` — so
rust-analyzer canonicalises `crate::config::DaemonConfig` as `tddy_daemon::config::DaemonConfig` and
the check reads a re-export as an origin dependency. The fifth,
`tddy_daemon::screen_sharing_vault::`, is worse in kind: that module had **already moved to the
destination crate** in the same milestone, so the path the operation objects to is one that resolves
*into the crate it is being asked to move the caller to*.

What this costs, concretely: the module has to be hand-re-pointed at the real crates before the
operation will look at it — and once it has been, the operation's remaining value is the `git mv`
and two manifest edits, which is why it was hand-moved instead. Two suggestions on top of the fix
already recorded above:

- **Resolve a re-export to its defining crate before deciding.** rust-analyzer knows
  `tddy_daemon::config` is `tddy_daemon_kernel::config`; the refusal should be raised against the
  definition site, not the canonical path through the facade.
- **Exempt paths that resolve into the destination.** A module naming a sibling that has already
  landed in the destination is the *normal* mid-plan state of a multi-module extraction, not a
  cycle.

### No vocabulary for a seam split (`#unbundle` node 2, M4)

M4 set out to move the nine telegram modules and moved **part of one**: the transport half of
`telegram_notifier.rs` became `tddy-telegram/src/sender.rs`. `move_module_to_crate` was not
invoked, and the reason is structural rather than a bug — recording it here because it is the
shape the *remaining* daemon extractions will keep hitting.

The operation's unit is a whole module: an anchor `<crate>/src/<module>.rs`, moved entire. But when
a subsystem's centre cannot leave — and telegram's centre cannot; see the changeset — the only
extraction available is a **seam**: a contiguous run of items inside a module, lifted out while the
rest stays and re-exports it. The plan schema has no way to say "lines 78–99 and 111–291 of this
module", so there was nothing to hand the operation.

Had it been handed `telegram_notifier.rs` whole it would also have been refused on the
already-recorded facade-cycle check — it names `crate::config` (node 1's `pub use`),
`crate::telegram_session_control`, `crate::active_elicitation` and `crate::telegram_tracked_session`
— but that refusal would have been beside the point, because moving the module whole was never
the operation wanted.

Worth considering for the operation, in rough order of value:

- **An item-list anchor.** `move_items_to_crate { from: <module>, items: [...] }`, leaving a
  `pub use` in the source module so callers do not move. That is exactly what M4 did by hand, and
  it is mechanical: resolve the items, carry their imports, emit the re-export.
- **Report the seam rather than the refusal.** When a module cannot move because *k* of its items
  reach the origin crate, the useful output is which items those are — because the complement is
  the movable seam. `telegram_notifier.rs` was 3 of its ~30 items away from moving whole.
