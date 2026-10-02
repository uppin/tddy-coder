# 2026-10-02 — Plans are loaded once, executed by reference and written back

`#live-plan` 2/7, PR [#538](https://github.com/uppin/tddy-coder/pull/538). Parent:
[#537](https://github.com/uppin/tddy-coder/pull/537). Successors: [#539](https://github.com/uppin/tddy-coder/pull/539),
[#540](https://github.com/uppin/tddy-coder/pull/540).

A restructuring plan used to be re-read from disk by every `check`, `apply` and `status`, and was never
rewritten, so anchors an earlier operation of the same plan had moved past stayed stale in the file. A
**plan store** now holds the plan: operations carry stable ids, the applied plan's pending anchors are
rewritten after every operation, and the plan is written back, so a `--resume` or `--from` reads anchors
that match the tree.

- **Operation ids** (`"id":"op-3"`), assigned on load, written back, refused when duplicated. The journal
  and apply events name operations by id; `--from` accepts an index or an id (without a daemon).
- **`restructure load`, `unload`, `plans`** and the daemon's `LoadPlans`, `UnloadPlans`, `ListPlans`.
  `Apply` loads a plan that is not loaded. Without a daemon the three are refused as needing one, and
  `apply` and `check` run over a store that lives for the invocation.
- **Write-back** within a second of a change, synchronously after each operation, on unload, at run end,
  and on `SIGTERM`/`^C` of a served daemon. A plan whose file changed since it was loaded is never
  overwritten.
- **A continued run is verified**: the plan must match a digest the run journalled, so a crash between an
  operation and its write-back is refused (`PlanOutOfSync`) rather than redone or skipped. Item anchors
  now resolve on a continued run, replacing the previous refusal. A journal from before write-back
  resumes a plan of ranges and symbols as it always did, unnumbered and not written back (kept with the
  developer's consent); over an item-anchored plan it is refused (`PlanUnverifiable`).
- **A second plan under a root where a first completed through the daemon applies without `--resume`**:
  the daemon's apply loop uses plan-scoped run state.

Limits: `--from <id>` does not reach the daemon (`ApplyRequest` has no field for it); the crash window is
detected, not repaired. See [Rust code restructuring](../rust-code-restructuring.md#plan-store) and the
[warm daemon](../warm-code-intelligence-daemon.md#plans-the-daemon-holds).
