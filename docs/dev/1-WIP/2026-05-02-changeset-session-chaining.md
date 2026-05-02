# Changeset: Session chaining — Phase 2 follow-ups

**Date**: 2026-05-02  
**Status**: In progress (Phase 2 — live Telegram wiring + parity)  
**Type**: Feature

## Phase 1 (shipped) — permanent docs

Session chaining **core**, **`.session.yaml`** **`previous_session_id`**, worktree bootstrap integration, Telegram **harness** (**`handle_chain_workflow`**, parent picker, **`handle_chain_parent_callback`**), **`/chain-workflow`** on the **message** path in **`telegram_bot`**, TUI **`/chain`** slash row, and acceptance tests are described under:

- [Telegram session control](../../ft/daemon/telegram-session-control.md) — includes **live vs harness** note for **`tcp:`** callbacks  
- [Git integration base ref](../../ft/coder/git-integration-base-ref.md) — **`repo_path`** rule when parent has a branch  
- [Session layout](../../ft/coder/session-layout.md)  
- [Feature prompt: agent skills and slash](../../ft/coder/feature-prompt-agent-skills.md)  
- [Coder changelog](../../ft/coder/changelog.md), [Daemon changelog](../../ft/daemon/changelog.md)  
- [tddy-core changesets](../../packages/tddy-core/docs/changesets.md), [tddy-daemon changesets](../../packages/tddy-daemon/docs/changesets.md), [dev changesets index](../changesets.md)

## Phase 2 — remaining work

### Product

- [ ] **`telegram_bot`**: dispatch **`CallbackQuery`** **`tcp:`** (**`parse_telegram_chain_parent_callback`**) → **`handle_chain_parent_callback`** for long-polling operators.  
- [ ] After parent pick: carry resolved chain integration base through **project / branch / spawn** without dropping operator overrides.  
- [ ] **TUI**: session picker + worktree bootstrap parity with Telegram **`/chain-workflow`**.  
- [ ] Optional: stable parent **session id** in **`callback_data`** (vs list index) when sessions churn.

### Tests / refactor (non-blocking)

- [ ] Shared Telegram integration fixtures in **`packages/tddy-daemon/tests/common.rs`** if chain tests keep growing.  
- [ ] Optional unit tests for **`parse_chain_workflow_prompt`**.  
- [ ] Test helper / **`Changeset`** + session-dir builders (**from @red** ergonomics).

## Validation (short)

- Last **`cargo fmt` / `clippy --workspace --all-targets -D warnings`**: pass on branch.  
- Scoped tests: **`session_chain_acceptance`**, **`telegram_chain_workflow_dispatch_acceptance`**, **`telegram_session_control_integration`** chain cases, **`tui_chain_slash_offers_recent_sessions`**.  
- Full **`./dev bash ./verify`**: may fail **`tddy-e2e`** install test when **`/usr/local/bin`** is not writable (environment).
