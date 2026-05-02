# Changeset: Session chaining (stacked PR groundwork)

**Date**: 2026-05-02  
**Status**: In progress (implementation slice; product wiring continues)  
**Type**: Feature

## Affected packages

- `tddy-core` — `session_metadata`, `session_chain`, `agent_skills`, `lib` exports
- `tddy-daemon` — `telegram_session_control`
- `tddy-tui` — `view_state` (slash menu test)
- `tddy-integration-tests` — `session_chain_acceptance`
- `tddy-coder`, `tddy-service`, `tddy-daemon` tests — `SessionMetadata` / `InitialToolSessionMetadataOpts` literals

## Related feature documentation

- [Telegram session control](../../ft/daemon/telegram-session-control.md)
- [Git integration base ref](../../ft/coder/git-integration-base-ref.md)
- [Session layout](../../ft/coder/session-layout.md)
- [Feature prompt: agent skills and slash](../../ft/coder/feature-prompt-agent-skills.md)

## Summary (State B)

Optional **session chaining** lets a workflow session record a **parent session id** in **`.session.yaml`** and resolve a **chain-PR integration base** (`origin/<branch>`) from the parent session’s **`changeset.yaml`** branch (or branch suggestion), validated with **`validate_chain_pr_integration_base_ref`**, with **`repo_path` required on the parent changeset whenever a branch is present** so the child project repository can be aligned. **`integrate_chain_base_into_session_worktree_bootstrap`** applies that ref through **`setup_worktree_for_session_with_optional_chain_base`**.

**Telegram**: **`TelegramSessionControlHarness::handle_chain_workflow`** creates a child session, lists other sessions (newest first), sends a **parent picker** first (inline **`tcp:<parent_idx>|s:<child_session_id>`** within Telegram **`callback_data` size limits**), then the standard recipe keyboard. **`parse_telegram_chain_parent_callback`** decodes **`tcp:`** payloads. **`handle_chain_parent_callback`** persists **`SessionMetadata.previous_session_id`** on the child (creating **`.session.yaml`** via **`write_initial_tool_session_metadata`** when absent). **`telegram_bot`** routes **`/chain-workflow`** through **`parse_chain_workflow_prompt`** → **`handle_chain_workflow`** (same authorization semantics as **`/start-workflow`**).

**TUI**: The feature slash menu includes **`/chain`** (same **`SlashMenuEntry::StartRecipe`** shape as **`/start-…`** rows).

## Outstanding (tracked for follow-up)

- **`tcp:`** callback routing in **`telegram_bot`** (harness **`handle_chain_parent_callback`** implements persistence; long-polling callback path still needs wiring for operator taps outside integration tests).
- Thread resolved chain integration base through the remainder of the Telegram workflow (project / branch / spawn) after parent pick without dropping operator overrides.
- TUI session picker and bootstrap parity with Telegram.
- Optional: stable parent id encoding vs list index for concurrent session churn.

## Acceptance tests (reference)

- `chain_child_metadata_records_previous_session_id` (`tddy-core` `session_metadata` tests)
- `chain_base_resolved_from_parent_session_changeset_branch`, `chain_rejected_when_parent_has_no_branch` (`tddy-integration-tests` `session_chain_acceptance`)
- `chain_rejects_when_parent_changeset_omits_repo_path` (`tddy-integration-tests` `session_chain_acceptance`) — **Green (2026-05-02)**: `repo_path` required when parent has a branch; [`resolve_chain_integration_base_ref_from_parent_session`] returns **`WorkflowError::ChangesetInvalid`** with operator-facing copy.
- `telegram_chain_workflow_shows_parent_pick_first` (`tddy-daemon` `telegram_session_control_integration`)
- `telegram_bot_rs_dispatches_chain_workflow_command` (`tddy-daemon` `telegram_chain_workflow_dispatch_acceptance`) — **Green (2026-05-02)**: `telegram_message_handler` branches on **`parse_chain_workflow_prompt`** and calls **`handle_chain_workflow`** when authorized.
- `telegram_chain_parent_tap_persists_previous_session_id_on_child` (`tddy-daemon` `telegram_session_control_integration`) — **Green (2026-05-02)**: **`handle_chain_parent_callback`** rebuilds the parent candidate page like **`handle_chain_workflow`**, validates index, writes/merges **`.session.yaml`** with **`previous_session_id`**.
- `tui_chain_slash_offers_recent_sessions` (`tddy-tui` `view_state` tests)
- `session_chain` unit tests in `tddy-core`

### Refactoring needed (from @red acceptance pass)

- [ ] Extract shared Telegram integration fixtures (`create_fake_sessions`, harness wiring) into `packages/tddy-daemon/tests/common.rs` if chain tests multiply further.
- [ ] Add focused unit tests for `parse_chain_workflow_prompt` (optional; integration + source contract may suffice until Green).

### From @red (TDD Red Phase)

- [ ] Test helper function needed for repeated setup (chain workflow + parent sessions across integration tests)
- [ ] Test data builder would improve readability (parent `Changeset` + child session dir seeds)
- [ ] Mock factory not required today; `InMemoryTelegramSender` + harness remain sufficient

## Validation Results

**Last run**: 2026-05-02 (PR-wrap re-run: validate + `cargo fmt` check + full `clippy` + `./dev bash ./verify`)

### Change validation (@validate-changes)

**Status**: Passed (scoped feature slice; follow-ups remain documented below)  
**Risk level**: Low–medium — chain base resolution touches git/worktree bootstrap and Telegram callback sizing; no secrets or unvalidated remote execution in the new core APIs.

**Changeset sync**: Implementation matches **Summary (State B)** for core resolution, bootstrap integration hook, Telegram parent-picker-first harness path, and TUI `/chain` menu row. **Ensure `git add`** for previously untracked: `packages/tddy-core/src/session_chain.rs`, `packages/tddy-integration-tests/tests/session_chain_acceptance.rs`, and this WIP doc before opening the PR.

**Open gap (documented in Outstanding)**: **`telegram_bot`** message path dispatches **`/chain-workflow`**; **`tcp:`** inline keyboard callbacks for parent pick are **not** wired in the live callback handler yet — harness-only until that lands.

**Clippy / build**: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass (Rust 1.94–style lints already addressed in this branch).

### Test validation (@validate-tests)

**Status**: Passed (scoped packages)

**Summary**: Session chain and Telegram harness tests use temp dirs, git fixtures, and concrete assertions; no ignored or focused tests in touched files.

**Command (pass)**:  
`./dev bash -c 'cargo test -p tddy-core -p tddy-daemon -p tddy-tui -p tddy-integration-tests -p tddy-coder -p tddy-service -- --test-threads=1'`

**Note**: Full `./dev bash ./verify` (entire workspace, output in `.verify-result.txt`) reports **one failure** in `tddy-e2e` **`install_succeeds_without_codex_acp_native_when_not_required`** — install script defaults **`BIN_DIR=/usr/local/bin`** and hits **permission denied** when removing/writing there in unprivileged CI/agent environments; **not attributed to session-chain changes**. Re-run on a host with writable install targets or adjust test env per `tddy-e2e` harness expectations.

### Production readiness (@validate-prod-ready)

**Status**: Acceptable for merge as an incremental slice; known follow-ups below.

- **FIXME** (existing): `telegram_session_control.rs` — plan review path still has a FIXME for branching approve/reject from live callback data (~2401).
- **Preflight**: Remove or omit from commit **`.red-session-chain-test-output.txt`** (untracked scratch) if present.

### Code quality (@analyze-clean-code)

**Score**: ~8/10 for `session_chain` — small focused functions, named constant for operator-facing copy, logging at appropriate levels; `resolve_*` is intentionally linear (repo alignment + ref validation).

### Final re-validation (@validate-changes)

No new architectural risks in the session-chain slice; **`tcp:`** bot wiring remains the main product gap vs harness (tracked under Outstanding).

### Linting & format (step 6)

- `cargo fmt --all --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK
- `cargo test` via `./dev bash ./verify` — **1 failure** (`tddy-e2e` install path permissions); all other packages in run completed before that failure (see `.verify-result.txt` tail)

### Documentation wrap (@wrap-context-docs)

**Status**: **Blocked** — **Outstanding (tracked for follow-up)** in this doc is non-empty (`tcp:` callback routing in **`telegram_bot`**, chain base through workflow, TUI parity, etc.). Per project rules, **do not** delete this WIP changeset or fold into permanent docs until scope is complete and checkboxes allow wrapping.
