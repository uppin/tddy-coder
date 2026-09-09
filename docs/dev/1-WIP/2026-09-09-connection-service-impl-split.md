# Changeset: move every `impl` block out of `connection_service.rs`

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Refactor

## Responsibility

`packages/tddy-daemon/src/connection_service.rs` **16,659 → 2,634 lines**: every `impl` block and the
free-item families to their own modules under `connection_service/`. Plus the six restructuring-tool
defects that had to be fixed to do it.

Predecessor: **#467** (merged as `a0d07c14`), which moved the 29 test modules and repaired the
bridge-transport handshake. This node's defects were only *observable* once assists worked at all.

## Boundaries

- Does **not** change behaviour. Every moved character came from rust-analyzer; the hand-written
  lines are `impl` block boundaries, restored imports and visibility widenings.
- Does **not** touch anything outside `connection_service*` and the restructuring packages.
- Does **not** finish the sub-500 goal — see *Deferred*.

## Technical Changes

### State B

| | |
|---|---:|
| `connection_service.rs` | 2,634 |
| modules under `connection_service/` | 59 |

13 hand-inserted `impl` boundaries cut the four large inherent impls into file-sized blocks; 22
`extract_module` operations then moved every block — 17 inherent, 5 trait — to its own module, plus 8
free-item families. Moving a whole `impl` is free of caller churn, so those need no facade; the
free-item families carry `reexport: "glob"`, leaving the 93 files that name their symbols untouched.

### Tool defects fixed

| | Defect | Presented as |
|---|---|---|
| Aliased import unreconstructable — blocked every seam naming a generated proto type | a plan defect |
| Module binding (`use crate::x;`) not offered at all — three modules referenced an unlinked crate | success |
| A contested name unsettleable once the parent's binding was pruned | an ambiguity refusal |
| The import pass added a private named import that shadowed the facade it was given | success, then `E0603` |
| `pub use` glob over a module publishing nothing | a clippy error |
| A rewrite written over itself (`SeededCloneGuardloneGuardloneGuard`) | success |

Plus two caps mistaken for guarantees: `IMPORT_PASSES` 64 → 512, and the per-operation settle budget
now derived from `--indexing-budget`.

## Verification

- `cargo test -p tddy-daemon --lib` — 809 passed / 0 failed, matching the recorded baseline
- `cargo test -p tddy-code-restructuring` — 248 passed / 0 failed (26 added across the tooling packages)
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` — clean
- Moved-line diffs in the commit messages; the boundary insertion is a **zero-moved-line** diff
  (0 lost, 26 gained: 13 `}` + 13 `impl ConnectionServiceImpl {`)

## Deferred — seven files still over 500 lines

| File | Lines | Why |
|---|---:|---|
| `rpc_service.rs` | 6,842 | **Out of scope, permanently.** The trait impl floors at ~680: 111 members (90 RPCs + 21 stream associated types), 385 lines of signatures alone, and a trait impl cannot be split across blocks. Going lower needs a delegation macro, which costs go-to-definition and grep on RPC names — the wrong trade for a 90-RPC service. Below 500 would need the `.proto` service split, which is a protocol change |
| `host_add_key_handler_tests.rs` | 1,264 | Deferred. Nesting them into submodules produced 80 compile errors: a grandparent's `use super::*` glob is not transitively re-exported to a nested child, and a `pub use` facade fails because the items are only `pub(crate)` |
| `agent_activity_unit_tests.rs` | 1,234 | same |
| `svc_start_session_core.rs` | 878 | Deferred. One 830-line method; needs `extract_method`, which needs full type inference and timed out at 300s. Plausible now the file is 6× smaller |
| `svc_start_sandboxed_claude_cli_session.rs` | 653 | same, one 606-line method |
| `svc_resolve_tddy_tools_path.rs` | 581 | Reachable — many small methods, cuttable with a lower boundary target |
| `svc_resolve_os_user.rs` | 506 | same |

Records: [`docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md`](../todo/2026-09-09-restructure-defects-from-the-connection-service-split.md),
[`…-macro-expansion-as-a-restructure-operation.md`](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md),
[`…-keeping-target-from-overhogging-the-disk.md`](../todo/2026-09-09-keeping-target-from-overhogging-the-disk.md).

## Final Checklist

- [ ] `packages/tddy-daemon/docs/changesets/` — release-note file with the before/after line counts
- [ ] `packages/tddy-code-restructuring/docs/changesets/` — the six defects (package has no `docs/` yet)
- [ ] `packages/tddy-daemon/docs/connection-service.md` § *Where the code lives* — replace the
      single-file description with the module map, and say where a new RPC handler goes
- [ ] `docs/ft/coder/rust-code-restructuring.md` — the four import-restoration fixes not yet described
      there (alias, module binding, module agreement, facade interaction)
