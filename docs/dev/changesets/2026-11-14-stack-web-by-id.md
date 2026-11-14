# 2026-11-14 — The PR-stack screen opens a project stack by id

**Type:** Feature

`#orchestrators 9/11`, PR [#886](https://github.com/uppin/tddy-coder/pull/886). Feature:
[PR stacking § Opening a project stack by id](../../ft/coder/pr-stacking.md#opening-a-project-stack-by-id),
[Session drawer § Stacks row](../../ft/web/session-drawer.md#stacks-row).

## What changed

- **`tddy-service`** — `pr_stack.proto`: `StackAddress { project_id, stack_id, daemon_instance_id }`
  on the six `session_id`-addressed requests; `ListProjectStacks` / `GetProjectStack`;
  `ProjectStackSummary`, `StackNodeCounts`, `UnreachableStackHost`. `session.proto`:
  `SessionEntry.stack_id = 38`. TypeScript regenerated for both.
- **`tddy-pr-stack`** — `PrStackHandler::{list_project_stacks, get_project_stack}`.
- **`tddy-daemon-rpc`** — `pr_stack/project_stacks.rs` (address resolution, forwarding, target
  resolution) and `pr_stack/project_stack_queries.rs` (the two RPCs, by-address store mutations);
  `ports.rs` delegates and authenticates before validating an address. A `session_id` resolves
  through its changeset's `stack_id` whatever its recipe; the listing fans out one hop and names
  unreachable hosts.
- **`tddy-session-activity`** — the session-list enrichment fills `SessionEntry.stack_id` from the
  changeset.
- **`tddy-prstack-web`** — `ProjectStackView`, the full-width stack view loading by address.
- **`tddy-session-web`** — the drawer Stacks row (`ProjectStacksRow`) and the stack detail case.
- **`tddy-client-web` / `tddy-web`** — the `#/sessions/project/<id>/stack/<stack_id>` route.

## Resolved backlog

None. The changeset's ⚠ DURING entries stay open: `QueryBranch`'s local-only legs
(`2026-08-30-pr-stack-querybranch-s-local-only-legs`), cheap-RPC tests
(`2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility`).

## Code issues

- `tddy-daemon-rpc/src/pr_stack/project_stacks.rs` split at wrap: 546 → 313 production lines
  (`move_impl_members` into `project_stack_queries.rs`, 246 lines; `restructure verify` clean).
- `oversized-file-pr-stack-ports.md` re-measured: 818 → 868 production lines — stack-shared with
  n10, split deferred to the post-stack sweep.
- `oversized-file-pr-stack-screen.md` re-measured: 701 → 731 lines.
- `oversized-file-sessions-drawer-screen.md` recorded new: 487 → 581 lines — stack-shared drawer
  file, split deferred to the post-stack sweep.
- `oversized-file-telegram-session-control-callbacks.md` re-measured: 686 → 687 (+1 literal).
