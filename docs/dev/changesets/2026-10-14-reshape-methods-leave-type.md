# 2026-10-14 — method bodies leave a type that stays: `read_fields_through` and `retarget_impl`'s delegator

**Type:** Feature

`#reshape` 6/19 (`feature/reshape/methods-leave-type`, PR #603). A new restructure operation,
`read_fields_through`, rebinds the `self` a range of a method reads to a local, so a following
`extract_method` writes a free function; `retarget_impl` implements `variant: "leave_delegator"`; and
`verify` accounts for both (`--rebind NAME`, rule R3 under `--retarget`).

## By package

- `tddy-code-restructuring`: the operation, the delegator, R3 and R-rebind, the `--rebind` flag. See its
  changeset of the same slug.
- `tddy-index-daemon`: `VerifyRequest.rebinds = 5`, carried by the daemon and its CLI. See its changeset of
  the same slug.
- `tddy-tools`: `index_client.rs::verify` carries `rebinds`.
- Config: `.config/nextest.toml` (`rust-analyzer` group) and `.config/rust-e2e.filterset` register the two
  new live test binaries.
- Feature doc: `rust-code-restructuring.md` (`## Rust operations (v1)`, `### retarget_impl`,
  `### read_fields_through`, `## Verify`, `## CLI`), and the `code-restructuring` agent skill's plan schema
  and recipe (operations: twenty-five to twenty-six).

## Backlog and code issues

- Resolved and deleted: `2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter`
  and `2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator`. The `pub`
  wrapper case moved to `2026-10-09-restructure-dead-pub-wrappers-are-not-reported`, which stays.
- Measurements and the deferral of the `verify/retarget.rs` split (341 → 555 production lines, over the
  500 budget, consent 2026-10-14, owned by the stack's later nodes): see the package changeset. Entry
  `2026-10-14-restructure-verify-retarget-rs-exceeds-the-500-line-budget` and
  `oversized-file-verify-retarget.md` carry it.
- Downstream: `#reshape` 18/19 (`feature/reshape/backend-session`) reuses the self-mode rewrite and the
  shadowing refusal from its own `detach_method`.
