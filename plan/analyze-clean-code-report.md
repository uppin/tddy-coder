# Clean-code analysis — web-dev daemon-only refactor scope

## Executive summary

The refactor keeps a clear separation between **granular predicates** (`contains_*`, `has_*`, `defaults_*`) and **orchestrating verifiers** (`verify_*`), with integration tests in `web_dev_script.rs` acting as a thin acceptance layer. The main quality gaps are **duplication** between `#[cfg(test)]` granular tests and the `verify_*` functions (overlapping concerns but not always calling the same entry points), **naming drift** (integration test name promises shellcheck though only `bash -n` runs; script header still refers to `CONFIG` for a path that is semantically daemon config), and **brittle substring contracts** (already noted in the evaluation report). The `web-dev` script is readable and linear; remaining complexity sits in startup/wait/cleanup, with some repeated patterns (double HTTP probe, port cleanup). Unrelated **rustfmt-only** edits in `livekit_terminal_rpc.rs` and `virtual_tui.rs` add review noise and should stay out of the feature changeset per `plan/evaluation-report.md`.

## Strengths

- **Module documentation** (`web_dev_contract.rs`, `web_dev_script.rs`): Explains purpose (static contract, PRD alignment, no servers) and points to the integration/delegation pattern.
- **Layering**: Predicates are pure on `&str` where possible; `verify_*` compose file I/O, `bash -n`, and predicates—reasonable **single responsibility** for a small contract module.
- **Integration tests** are short, intention-revealing names for PRD bullets (`always_targets_tddy_daemon_binary`, `default_config_is_dev_daemon_yaml`).
- **`web-dev` script**: Header documents usage and env vars; `set -euo pipefail`, `find_binary`, and the sed/temp-config path are localized and easy to follow.
- **Observability in tests**: `log::` at info/debug in verifiers aids debugging without polluting TUI production paths (these are test/contract-only modules).

## Issues

### Naming

- **`web_dev_script_passes_shellcheck_or_syntax_check`**: Name implies **shellcheck** or a syntax check; implementation only runs **`bash -n`** via `verify_syntax_and_no_legacy_branch`. Renaming or adding shellcheck (if desired) would align name and behavior.
- **`web-dev` variable `CONFIG`**: Holds the resolved **daemon** YAML path (`DAEMON_CONFIG` with default `dev.daemon.yaml`). The name overlaps the generic notion of “app config” and differs from the header’s mental model (“DAEMON_CONFIG”); readers may confuse it with legacy `dev.config.yaml` demo stack. Comments partially mitigate; the mismatch is cognitive load, not a test failure.
- **`contains_legacy_daemon_env_gate`**: “Gate” suggests a branch; the check is substring presence—accurate enough but could be read as implying control-flow analysis.

### Complexity

- **`web-dev`**: The daemon readiness loop (180 iterations, double 200 check) and cleanup/trap logic are the densest regions; they are imperative but justified for race avoidance. No unnecessary abstraction, but **high line count in one file** if future edits add more branches.
- **`web_dev_contract`**: Low cyclomatic complexity; `verify_*` are linear sequences of assertions.

### Duplication

- **Between `verify_*` and `granular_tests`**:  
  - `legacy_daemon_env_gate_absent_including_bash_syntax` duplicates `verify_syntax_and_no_legacy_branch` (same `bash -n` + legacy gate assertion) instead of calling the verifier.  
  - `tddy_demo_paths_absent` covers only the negative half of `verify_daemon_binary_only` (no `tddy-daemon` positive assertion).  
  - `dev_config_yaml_not_defaulted` covers only part of `verify_default_dev_daemon_config` (no positive `DAEMON_CONFIG:-dev.daemon.yaml` check).  
  So granular tests are **not a thin wrapper** around `verify_*`; they are a **partially overlapping** matrix—risk of drift if one side changes.
- **Path helpers**: `repo_web_dev_path` / `read_repo_web_dev` in `granular_tests` mirror `repo_root` / `web_dev_path` / `read_web_dev` in the integration test file—same repo layout assumptions, duplicated in two modules.

### SOLID (brief)

- **S**: Good—detectors vs orchestrators vs integration tests are separate.
- **O**: **Substring** predicates (`contains`, `CONFIG:-`) are brittle; extending the script with harmless comments or splitting strings could break tests without behavior change—favor **narrower matchers or structured checks** later if this becomes noisy.
- **L/D**: Integration tests depend on `tddy_e2e::web_dev_contract`; contract module does not depend on integration tests—direction is good.
- **I**: Not much interface surface; `verify_*` returning `()` + `assert!` is appropriate for test helpers but is **not reusable** from non-test code that might prefer `Result`—acceptable for current scope.

### Documentation gaps

- Integration test **`web_dev_script_passes_shellcheck_or_syntax_check`**: Doc says “shellcheck **or** syntax check”; only syntax is enforced—**align doc + name + tooling**.
- **`web_dev_contract`**: The top-level module doc could briefly state that matchers are **substring-based** and sensitive to comments (cross-reference evaluation-report suggestion).
- **`web-dev`**: Evaluation report already flags **duplicate `-c`** when users pass `-c` in `$@`; worth a one-line warning in the header if pass-through remains.

### Unrelated noise (scope hygiene)

- Per **`plan/evaluation-report.md`**: `packages/tddy-e2e/tests/livekit_terminal_rpc.rs` and `packages/tddy-tui/src/virtual_tui.rs` show **rustfmt-only** churn unrelated to the web-dev PRD—increases review burden and merge conflicts. Prefer revert or isolate to a formatting commit.

## Suggestions

1. **Unify granular tests with `verify_*`**: Prefer `granular_tests` calling `verify_syntax_and_no_legacy_branch(&path)`, `verify_daemon_binary_only(&contents)`, and `verify_default_dev_daemon_config(&contents)` to remove duplication—or drop redundant granular tests if integration tests already cover them (trade-off: unit vs integration locality).
2. **Rename** `web_dev_script_passes_shellcheck_or_syntax_check` to reflect **`bash -n` only**, or introduce optional `shellcheck` when available without weakening CI.
3. **Extract shared repo `web-dev` path** to a single test helper (e.g. small `#[cfg(test)]` module or `const` path builder) used by both `web_dev_contract::granular_tests` and `web_dev_script.rs` if duplication persists.
4. **Script clarity**: Consider renaming shell variable `CONFIG` to something like `DAEMON_CONFIG_PATH` while keeping env behavior documented, **only if** the team accepts churn in docs/scripts—purely readability.
5. **Matchers**: If false positives from comments appear, switch to line-based or regex boundaries for legacy tokens.

## Optional refactor backlog

| Item | Effort | Note |
|------|--------|------|
| Deduplicate granular vs integration coverage | Low | Call `verify_*` from `granular_tests` or consolidate test modules |
| Rename misnamed integration test + fix module doc | Trivial | Shellcheck vs `bash -n` |
| Shared `web_dev` path helper for e2e tests | Low | Reduces drift |
| Narrow legacy-token matching (avoid comment false positives) | Medium | If evaluation-report concern materializes |
| Revert/split rustfmt-only livekit/virtual_tui changes | Low | Cleaner PR |
| Document duplicate `-c` pass-through in `web-dev` header | Trivial | User confusion per evaluation report |

---

*Scope: `packages/tddy-e2e/src/web_dev_contract.rs`, `packages/tddy-e2e/tests/web_dev_script.rs`, repo-root `web-dev`; cross-reference `plan/evaluation-report.md` for rustfmt noise and hygiene items.*
