# 2026-11-21 — An orchestrator checks and refreshes its base, and a start reports the commit it was cut from

**Type:** Feature · **Stack:** `#orchestra` 4/9 (PR #982) · **Packages:** `tddy-worktree-service`, `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-agent-launch`, `tddy-session-files`, `tddy-service`, `tddy-tools` ·
Feature: [project-concept.md § Keeping the base fresh](../../ft/daemon/project-concept.md#keeping-the-base-fresh)

## Problem

A home worktree was fast-forwarded only when its orchestrator started or resumed. After that it planned
from whatever it had, and nothing recorded which base a child was cut from.

## What changed

- **`tddy-worktree-service::base_freshness`** (new): `base_freshness` (fetch the trunk, report commit, tip,
  ahead, behind), `refresh_home` (fast-forward only; `RefreshRefusal::Diverged` leaves a diverged home
  untouched), `CutBase::of_worktree` and `branch_of_worktree`. `home_worktree::{fetch_ref, git}` became
  `pub(crate)` so the module reuses them.
- **`tddy-tool-engine::base_freshness_tools`** (new): `CheckBaseFreshness` / `RefreshHome` definitions, the
  `BaseFreshnessPort` the daemon implements, `execute_base_freshness_tool`. Offered to orchestrator roles
  by the catalogue, the read-only allow list and `tddy-tools`' remote admission.
- **`tddy-daemon-rpc::exec_tool::base_freshness`** (new): the port over the caller's own home worktree and
  its project's trunk; the role check reads the caller's `.session.yaml` and refuses a worker.
- **Start ledger:** `StartLedgerRecord.{base_commit, base_behind}`, filled from the start's
  `worktree_ready` event when it carries a commit, so `GetSessionStart` reports them.
- **`StartWorktreeReady.{base_commit, base_behind}`** (proto fields 3, 4). `tddy-agent-launch` now emits
  `worktree_ready` for a **local** cut too (it was a target-only event), reading the base from the worktree;
  a cut on a target reads it through the target's shell with the same git invocations.

## Decisions

- **Both orchestrator roles** get the tools; they share `is_orchestrator_role`.
- **Reporting only.** A start from a stale base is not refused; the orchestrator decides when to look.
  Nothing polls the remote (n5).
- **`base_ref` was not added.** The PRD named it; the trunk ref is on `CheckBaseFreshness`'s answer and the
  recorded base is the commit.
- A branch with no upstream reads zero behind; an unreadable `HEAD`, branch or count fails the start's cut
  rather than reporting an empty base.
- Validation cleanup: `BaseFreshnessPort`'s methods are required (the refusing defaults were removed); the
  local cut's branch read reuses the worktree service's git helper and propagates its error.
- `start_ledger.rs` reached 501 production lines, so the event-folding arm moved, engine-driven
  (`restructure move_item`, `check --deep` clean): `apply_progress` and `step_of` now live in
  `start_ledger/progress_fold.rs`. `start_ledger.rs` is 440 production lines, `progress_fold.rs` 70.
- The peer-host gap is recorded as
  `docs/dev/todo/2026-11-21-base-freshness-tools-are-not-forwarded-to-a-peer-host.md`.

## Tests

`packages/tddy-daemon-rpc/tests/orchestrator_base_freshness_acceptance.rs` (3, through the real
`ExecuteTool`), `packages/tddy-worktree-service/tests/{base_freshness_acceptance,cut_base_acceptance}.rs`
(7, real git), `packages/tddy-tool-engine/tests/start_ledger_base.rs` and the tool module's unit test.
