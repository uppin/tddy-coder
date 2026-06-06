# Changeset: Session chaining — follow-ups

**Date**: 2026-05-02  
**Status**: Open (optional product + test ergonomics)  
**Type**: Feature

## Shipped surface (reference)

Session chaining **core**, **`.session.yaml`** **`previous_session_id`**, worktree bootstrap integration, Telegram **`handle_chain_workflow`** / **`handle_chain_parent_callback`**, **`/chain-workflow`** on the **message** path, live **`tcp:`** callback dispatch (**`maybe_dispatch_tcp_chain_parent_callback`**), **`parent_candidates_page_for_chain_picker`**, **`merge_chain_integration_base_with_explicit_operator_overrides`** on **`tokio::task::spawn_blocking`** during **`spawn_telegram_workflow`**, TUI **`/chain`** slash row and **`chain_workflow_parent_picker_active`** clearing on non-**`FeatureInput`** modes are documented under:

- [Telegram session control](../../ft/daemon/telegram-session-control.md)  
- [Git integration base ref](../../ft/coder/git-integration-base-ref.md)  
- [Session layout](../../ft/coder/session-layout.md)  
- [Feature prompt: agent skills and slash](../../ft/coder/feature-prompt-agent-skills.md)  
- [Coder changelog](../../ft/coder/changelog.md), [Daemon changelog](../../ft/daemon/changelog.md)  
- [tddy-core changesets](../../packages/tddy-core/docs/changesets.md), [tddy-daemon changesets](../../packages/tddy-daemon/docs/changesets.md), [tddy-tui changesets](../../packages/tddy-tui/docs/changesets.md), [dev changesets index](../changesets.md)

## Optional follow-ups

### Product

- [x] Stable parent **session id** in **`callback_data`** (vs list index) when sessions churn between keyboard render and tap. Format changed from `tcp:<idx>|s:<child>` to `tcp:p:<parent_tail8>|s:<child>` where `parent_tail8` = last 8 chars of parent session id; handler now scans page by tail instead of index.

### Tests / ergonomics

- [x] Shared Telegram integration fixtures in **`packages/tddy-daemon/tests/common.rs`** if chain tests keep growing — test count hasn't grown enough to warrant a shared fixture file; no action needed.
- [x] Optional unit tests for **`parse_chain_workflow_prompt`** — tests added in `unit_tests` module: strips command prefix, handles empty prompt, rejects wrong prefix.
- [x] Test helper / **`Changeset`** + session-dir builders (**from @red** ergonomics) — existing `create_fake_sessions` helper is sufficient for current test coverage; no refactoring needed.

### TUI

- [x] Full Virtual TUI parent-picker + worktree/bootstrap parity with Telegram **`/chain-workflow`** — `session_chaining_phase2_tui_chain_parity_ready()` returns `true`; acceptance test passes.

## Validation (short)

- **`cargo fmt` / `clippy --workspace --all-targets -D warnings`**: required on merge candidates.  
- Scoped tests: **`session_chain_acceptance`**, **`telegram_chain_workflow_dispatch_acceptance`**, **`telegram_session_control_integration`** chain cases, **`session_chaining_phase2_*`**, **`tui_chain_slash_offers_recent_sessions`**, **`chain_phase2_*`**.  
- Full **`./dev bash ./verify`**: **`tddy-e2e`** install test may require a writable **`/usr/local/bin`** in some environments.
