# 2026-10-02 — Item anchors: a plan names an item, and a line is only a hint

**Type:** Feature

`#live-plan` 1/7, the stack's root — PR [#537](https://github.com/uppin/tddy-coder/pull/537),
`feature/live-plan/item-anchors`, base `master`. Successors start at
[#538](https://github.com/uppin/tddy-coder/pull/538). Product entry:
[2026-10-02-item-anchors.md](../../ft/coder/changelog/2026-10-02-item-anchors.md).

A plan anchor was a bare symbol name, resolved to the first outline node matching it anywhere in the
file, or an absolute line and column trusted verbatim. One unrelated PR made six `#carve` plans stale.
`item` and `items` anchors name the item by a crate-rooted path and carry a range relative to it; they
resolve once at run open through rust-analyzer's outline, and a fingerprint of the item's text refuses
an edit to the item itself. Schema v2 makes the snapshot header a per-file hint. `restructure anchors
--at` and `--items` emit the anchors; the daemon's `Anchors` RPC and `tddy-tools` carry them.

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [item-anchors](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-02-item-anchors.md) — anchor kinds, resolver, run-open resolution, v2 header, `anchors --at` |
| `tddy-index-daemon` | [item-anchors](../../../packages/tddy-index-daemon/docs/changesets/2026-10-02-item-anchors.md) — `AnchorsRequest.at`, `AnchorsResponse.anchor_json` |
| `tddy-tools` | [item-anchors](../../../packages/tddy-tools/docs/changesets/2026-10-02-item-anchors.md) — `--at` through the daemon client |
| `tddy-lsp` | [item-anchors](../../../packages/tddy-lsp/docs/changesets/2026-10-02-item-anchors.md) — `LspClient::root_uri` |

The skill `code-restructuring` and its `references/plan-schema.md` author with item anchors.

## Verification

`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools -p tddy-lsp` (scoped, as the
repo's verification rule asks): **1137 passed, 0 failed, 9 ignored**; `cargo clippy -p <each> -- -D
warnings` and `cargo fmt` clean. The rest of the workspace is CI's.

## Deferred

- **Not verified at repo scale (developer decision, 2026-10-02):** `anchors <file> --items A,B` on the
  warm and the cold path against a real workspace. The empty-outline wait is reproduced and fixed. The
  warm-path refusal (every name "not defined", 3 to 89 ms after the request) is inferred, not
  reproduced, because the fixtures answer `documentSymbol` at once; the cold path's `lsp server
  exited` is unaddressed. The record `broken-restructure-anchors-empty-outline` stays, narrowed, with
  #537's claim, and its remainder is unowned: it needs a repo-scale cold and warm run.
- **Item anchors across runs** belong to the plan store (#538 onward); a continued run refuses them.

## Code issues, final measurements

| Record | Before | After |
|---|---|---|
| `oversized-file-plan` (tddy-code-restructuring) | 433 production lines | **799** — created, open |
| `oversized-file-runner-entry-points` (tddy-code-restructuring) | 506 | **602** — created, open |
| `oversized-file-backends-rust` (tddy-code-restructuring) | 4,359 | **4,433** — regressed, open |
| `broken-restructure-anchors-empty-outline` (tddy-code-restructuring) | open, claimed | narrowed, open, claim kept |
| `dead-code-plan-filehint-modified` (tddy-code-restructuring) | none | created, open |
| `stale-repo-scoped-restructure-state-apply` (tddy-index-daemon) | `apply.rs:45` | unchanged |

The three oversized files are not split here because later `#live-plan` nodes touch them. Nothing
was closed, so no record was deleted.

Backlog entries filed for deferred findings that fit no code-issue category, all dated 2026-10-02:
`locate_symbol` waits on an empty outline with no deadline; a server that never sends
`experimental/serverStatus` leaves `settled_outline` waiting; `ItemChanged` names the item and its
file but not the operation; the daemon's anchors path resolves an item twice; a static `check` cannot
verify item anchors. The backlog entry `restructure snapshot cannot rebase a stale plan` (2026-09-24)
is answered in part and stays; the stack's third node claims it.
