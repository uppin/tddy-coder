# 2026-11-22 — The `#orchestra` sweep: one orchestrator-caller check, hermetic tests, deferrals recorded
**Type:** Refactor

`#orchestra` 11/11 (#999). No behaviour change: every refusal keeps its status code.

## What changed

- **tddy-daemon-rpc**: `orchestrator_caller` now lives once, in `exec_tool/project_home/caller.rs` beside `project_home_caller`. The copy in `exec_tool/session_files.rs` is removed; `base_freshness.rs`, `lsp_roots.rs`, `path_access.rs` and `session_files.rs` import the moved one. The kept wording is `base_freshness.rs`'s ("record"); the removed copy said "role", and no test asserted either.
- **tddy-tools**: `a_system_prompt_for_a_remotely_routed_agent_is_carried_rather_than_refused` (and its blank-prompt sibling) clear the `TDDY_REMOTE_*` variables for their duration, so they pass inside a tddy session.
- **tddy-rust-typescript-tests**: `echo-unary.test.ts` asks the OS for a free port instead of binding `8887`. Not run locally (needs a built `tddy-coder` and the `tddy-web` bundle); left to CI.
- **docs/dev/todo/**: six entries the stack deferred record the developer's consent (2026-11-22); none is resolved.

## Baseline

Recorded on `f309a4604`: `./test -p tddy-daemon-rpc` 692 passed / 8 failed, of which 7 are pre-existing macOS sandbox/seatbelt failures and 1 was load-induced (`a_session_started_by_an_orchestrator_records_it_before_its_agent_runs`, passes alone). The `tddy-tools` test was red with `TDDY_REMOTE_SESSION_ID=x`.
After: `tddy-daemon-rpc` 693 passed / 7 failed (the same macOS seven); `tddy-tools --lib` 125/125, both remotely-routed tests also green with `TDDY_REMOTE_SESSION_ID=x`; clippy `-D warnings` clean for both packages.

## Engine refusal and consent

`restructure check --deep` refused moving `orchestrator_caller` into `project_home::caller`: "plan is malformed: `mod project_stacks_unit_tests;` leads to neither `project_stacks_unit_tests.rs` nor `project_stacks_unit_tests/mod.rs`" (`pr_stack/project_stacks.rs` declares it with `#[path]`). Known gap: `docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`, which now records this instance. The developer consented to a hand move (2026-11-22); the steps were a cut and paste as `pub(in crate::exec_tool)`, dropping now-unused imports, and re-pointing the four importers.

## Analyzer not run

`/analyze-code-issues` was skipped at the developer's choice (2026-11-22; the coverage builds of about ten crates took hours on a loaded host). Nodes 5-8 did not run it either, so the code-issue debt of those crates is unmeasured, not proven zero.

## Validation

`/validate-changes`: no findings. The moved check is unchanged apart from the shared wording; no mock code, fallback or debug output was added. No permanent doc names `orchestrator_caller`, so none changed.
