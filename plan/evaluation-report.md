# Evaluation Report

## Summary

Reviewed working tree for web-dev daemon-only: core change is `web-dev` simplified to always run `tddy-daemon` with `DAEMON_CONFIG`/`dev.daemon.yaml`, plus new `tddy_e2e::web_dev_contract` and integration tests. Docs/changelog and `dev.daemon.yaml` header updated. Unrelated rustfmt-only edits appear in `livekit_terminal_rpc.rs` and `virtual_tui.rs` (noise). Untracked `.green-web-dev-test-output.txt` and `.red-web-dev-test-output.txt` look like agent artifacts and should not ship. `cargo check` passes for `tddy-e2e` and `tddy-tui`.

## Risk Level

medium

## Changed Files

- web-dev (modified, +26/−70)
- dev.daemon.yaml (modified, +1/−1)
- docs/ft/coder/changelog.md (modified, +5/−0)
- packages/tddy-e2e/src/lib.rs (modified, +1/−0)
- packages/tddy-e2e/tests/livekit_terminal_rpc.rs (modified, +29/−15)
- packages/tddy-tui/src/virtual_tui.rs (modified, +1/−2)
- packages/tddy-e2e/src/web_dev_contract.rs (added, +166/−0)
- packages/tddy-e2e/tests/web_dev_script.rs (added, +47/−0)

## Affected Tests

- packages/tddy-e2e/tests/web_dev_script.rs: added
  New integration tests: web_dev_script_passes_shellcheck_or_syntax_check, web_dev_always_targets_tddy_daemon_binary, web_dev_default_config_is_dev_daemon_yaml
- packages/tddy-e2e/src/web_dev_contract.rs: added
  New lib module with #[cfg(test)] granular_tests (3 tests) and verify_* used by integration tests
- packages/tddy-e2e/tests/livekit_terminal_rpc.rs: updated
  Formatting-only changes under #[cfg(feature = livekit)]; no test logic change

## Validity Assessment

The changes match the PRD: single backend (tddy-daemon), default `dev.daemon.yaml` via `DAEMON_CONFIG`, preserved sed temp config and CLI passthrough, removed legacy demo path and demo-only URL injection, documented CONFIG vs DAEMON_CONFIG in changelog. Automated static contract tests cover syntax and content; runtime E2E (daemon+Vite) remains manual per testing plan. Medium risk is expected user-visible breakage for anyone still relying on the removed tddy-demo default—mitigated by changelog note. Unrelated formatting files should be trimmed from the changeset for a cleaner merge.

## Build Results

- tddy-e2e: pass (cargo check -p tddy-e2e)
- tddy-tui: pass (cargo check -p tddy-tui (touched by formatting))

## Issues

- [low/hygiene] .: Untracked files `.green-web-dev-test-output.txt` and `.red-web-dev-test-output.txt` at repo root; add to .gitignore or delete before commit.
  Suggestion: Do not commit agent test output captures.
- [low/scope] packages/tddy-e2e/tests/livekit_terminal_rpc.rs: Large diff is rustfmt-only (import order, wrapping); unrelated to web-dev PRD; increases review noise and merge-conflict risk.
  Suggestion: Prefer isolating formatting to a dedicated commit or reverting if accidental.
- [info/behavior] web-dev: Pass-through `daemon_args+=("$@")` after `-c "$TMP_CONFIG"` can yield duplicate `-c` if the user passes `-c` on the CLI; Vite proxy still uses `DAEMON_PORT` from `CONFIG` env path (same as prior daemon branch).
  Suggestion: Document or resolve in a follow-up if users report confusion.
- [low/testing] packages/tddy-e2e/src/web_dev_contract.rs: Contract checks rely on substring predicates (e.g. no `TDDY_USE_DAEMON` anywhere); a future comment could trip the test.
  Suggestion: Keep comments free of legacy tokens or narrow matchers in a later refactor.
