# 2026-11-14 — `move_item` / `move_impl_members` refused in a crate whose module is declared with `#[path]`

**Category:** Known limitation (engine capability)

**Source:** #901 (`#orchestrators` 12/12, the sweep) — items B2 and the B6 remainder. Standing
records: `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md` (B2) and
`packages/tddy-tools/docs/code-issues/oversized-file-server.md` (B6).

## What happened

Both splits were proven with `restructure check --deep` against the warm index and refused at the
plan scan, on a module neither split touches:

- **B2, `tddy-daemon-rpc`:** `move_impl_members` / `move_item` out of
  `packages/tddy-daemon-rpc/src/pr_stack/ports.rs` (868 production lines) fail the plan scan on the
  crate's unrelated `#[path]` module (`mod project_stacks_unit_tests;`). The file stays at 868
  production lines (876 total at the sweep head).
- **B6 remainder, `tddy-tools`:** every `move_item` / `move_impl_members` plan for the remaining
  `server::turns`, `status`, `schemas`, `approval`, `dynamic_tools` and `tool_inputs` seams fails
  `check --deep` on that crate's unrelated `#[path]` module (`mod notify_cli_unit_tests;` in
  `notify_cli.rs`). The first subagent group did land via `extract_module`
  (`server::subagent_tools`, 296 lines); `packages/tddy-tools/src/server.rs` stays at 2,024
  production lines.

Under green-yolo consent an engine refusal is not done by hand. The developer accepted both as
standing debt on 2026-11-14, reason: "restructure engine refused" — recorded on each code-issue
record above.

## Related limitation: impl-member seams

The B6 `approval` seam is `impl PermissionServer` members, which `extract_module` cannot take at
all: the only engine path for it is `move_impl_members`, which the `#[path]` scan failure above
blocks. Even with the scan fixed, an `extract_module`-only crate would still have no way to move
impl members into a sibling module.

## What would close it

Engine fixes in `packages/tddy-code-restructuring`:

- The plan scan must not fail a whole plan on an unrelated `#[path]` module it cannot resolve:
  either resolve `#[path]` modules, or scan past them and report the gap against that module only.
- `move_impl_members` must be usable for impl-member seams such as `impl PermissionServer`, so a
  seam that is not a free-item group has an engine path.

Then re-anchor B2 and the B6 remainder with `tddy-tools restructure anchors` against a warm index
(`./run-index-daemon`), prove each seam with `restructure check --deep`, apply, and delete or
narrow the two records with their final measurements in the change-history entry.
