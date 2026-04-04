# Clean Code Analysis: Project Main Branch Ref (Integration Base)

**Scope:** `packages/tddy-core/src/worktree.rs`, `packages/tddy-daemon/src/project_storage.rs`, `packages/tddy-core/src/lib.rs` (worktree exports), integration tests in `packages/tddy-integration-tests/tests/worktree_acceptance.rs` and related daemon tests.

**Reference:** Aligns with findings in [evaluation-report.md](./evaluation-report.md) (PRD gap: `main_branch_ref` not threaded into session worktree setup; heuristic default resolution; double-fetch caveat).

---

## Summary

The feature area shows **clear naming** for integration-base concepts (`validate_integration_base_ref`, `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`, `effective_integration_base_ref_for_project`), **reasonable module cohesion** (validation in core, persistence and project resolution in daemon), and **useful module-level and public API docs**. Main quality gaps are **test and fixture duplication**, a few **stale or narrow docstrings**, **misleading test naming** relative to behavior, and **architectural incompleteness** (storage/API vs. call sites) rather than local code smell.

---

## `worktree.rs`

### Naming

- **Strengths:** `integration_base_ref` is used consistently for remote-tracking refs (`origin/<branch>`). `resolve_default_integration_base_ref` accurately describes behavior when no project override exists. `setup_worktree_for_session_with_integration_base` is explicit and pairs well with `setup_worktree_for_session`.
- **Minor drift:** `fetch_origin_master` still names the historical operation but delegates to `fetch_integration_base(..., DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF)` — accurate but readers may assume it only ever touches `master`; the doc on `fetch_integration_base` clarifies the general case.

### Complexity

- **`validate_integration_base_ref`:** Linear checks, single responsibility, easy to test and reason about.
- **`resolve_default_integration_base_ref`:** Sequential policy (master → main → `origin/HEAD`) with small helpers (`remote_ref_exists`). Complexity is appropriate for the policy; no deep nesting.
- **`create_worktree_with_retry`:** Bounded retry loop with clear exit conditions.

### Duplication

- **Unit tests (`integration_base_red_tests`):** Two tests duplicate a large block of git init / config / commit / remote / push setup. Extracting a local helper (e.g. `init_bare_style_repo_with_main_branch()`) would reduce duplication and shrink failure surface when fixtures change.
- **`slugify_for_worktree`:** Trivial wrapper around `slugify_for_branch`. Acceptable for naming clarity at call sites; could be inlined if the team prefers fewer lines.

### SOLID / boundaries

- **Single responsibility:** Validation, fetch, default resolution, worktree lifecycle, and changeset updates are separated into named functions. Session setup reuses `setup_worktree_for_session_with_integration_base` from `setup_worktree_for_session`, avoiding a second implementation path.
- **Dependency direction:** Core depends on `changeset` and `std::process::Command` only — no upward dependency on daemon or YAML. Appropriate.

### Documentation and comments

- Module and public items are generally well documented.
- **`create_worktree` doc** (lines 95–98) still emphasizes `Some("origin/master")` as the example; any validated `origin/<branch>` works. Updating the doc to say “remote-tracking ref such as `origin/main`” would match current behavior.
- **`remove_worktree`:** Comment explains directory fallback when `git worktree remove` fails; behavior is operational recovery, not an API “default substitute” for missing data.

### Consistency with repo style

- `Result<_, String>` for git operations matches existing patterns in this module.
- Logging uses `log` crate at appropriate levels; no TUI-unsafe `println!` in these paths.

---

## `project_storage.rs`

### Naming

- **`main_branch_ref` on `ProjectData`:** Stores a remote-tracking ref string; the field name matches YAML/proto vocabulary but can read as “branch name” rather than “integration base ref.” The serde field doc clarifies — acceptable given external contract.
- **`effective_integration_base_ref_for_project`:** Clear and describes the legacy default vs. override behavior.

### Complexity

- Thin layer: read/write/find, validation at `add_project` boundary, resolution helper. No unnecessary abstraction.

### Duplication

- **`validate_integration_base_ref`** is invoked in both `add_project` (when `Some`) and `effective_integration_base_ref_for_project` (when `Some`). Duplication is minimal; centralizing in one private `fn validated_clone(r: &str) -> anyhow::Result<String>` is optional polish.

### SOLID

- **Open/closed:** New fields on `ProjectData` use serde defaults; legacy YAML without `main_branch_ref` remains valid.
- **Dependency:** Correct use of `tddy_core::validate_integration_base_ref` and `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF` keeps rules in one place.

### Documentation

- Struct field docs and `effective_integration_base_ref_for_project` doc align with `tddy_core` terminology — good cross-crate consistency.

### Tests

- `project_integration_base_acceptance_tests` cover legacy default and invalid ref rejection with clear names. `projects_file_path` is used from the same module — fine.

---

## `lib.rs` exports (worktree)

### Naming and discoverability

- Re-exports are grouped under `pub use worktree::{ ... }` with a consistent list including `validate_integration_base_ref`, `resolve_default_integration_base_ref`, `setup_worktree_for_session_with_integration_base`, and `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`.
- Consumers can use either `tddy_core::resolve_default_integration_base_ref` or the module path — matches the crate’s established re-export style for other subsystems.

### Gaps (design, not style)

- Exports are sufficient for callers that will eventually wire `effective_integration_base_ref_for_project` into the worktree path; the missing piece is **usage** in daemon/workflow (per evaluation-report), not missing `lib.rs` symbols.

---

## Integration tests (`worktree_acceptance.rs` and daemon tests)

### Naming

- **`worktree_uses_configured_project_base_ref`:** The name implies `ProjectData` / `projects.yaml` drives the ref, but the test only exercises `setup_worktree_for_session` on a **main-only** remote so `resolve_default_integration_base_ref` picks `origin/main`. Per evaluation-report, this is a **naming / intent mismatch** — the test validates heuristic resolution, not persisted project configuration. Renaming (e.g. `worktree_resolves_origin_main_when_default_resolution_finds_main`) or extending the test to register a project would align name and behavior.

### Duplication

- **High:** Repeated blocks for git init, user config, first commit, remote add, push appear across many tests. A shared helper in the test module (or a small internal test support crate pattern) would improve maintainability and match DRY expectations for acceptance tests.

### Module-level docs

- The file header still states worktrees are created “from origin/master after plan approval.” After the feature, default resolution can be `origin/main` or `origin/HEAD` when `origin/master` is absent. Updating the bullet list avoids misleading new contributors.

### Daemon tests

- `acceptance_daemon.rs` and `multi_host_acceptance.rs` extend `ProjectData` with `main_branch_ref: None` — minimal, consistent with serde defaults.

---

## Cross-cutting: SOLID and layering

| Layer        | Responsibility                                      | Assessment |
|-------------|------------------------------------------------------|------------|
| `tddy-core` | Ref validation, fetch, default resolution, worktree | Cohesive; good separation from I/O format |
| `tddy-daemon` | YAML registry, effective ref for `project_id`      | Appropriate; does not duplicate validation rules |
| Call sites  | Session/worktree creation                          | Incomplete wiring (evaluation-report) — architectural gap, not a violation inside the edited modules |

---

## Recommendations (prioritized)

1. **Rename or extend** `worktree_uses_configured_project_base_ref` so the name matches what is proven (heuristic vs. registry-driven).
2. **Refresh** module docs in `worktree_acceptance.rs` and the `create_worktree` doc in `worktree.rs` for post-`origin/master`-only wording.
3. **Extract** shared git-fixture helpers in integration tests to cut duplication.
4. **Optional:** Single helper for “validate + clone ref string” in `project_storage` if `add_project` and `effective_integration_base_ref_for_project` stay in sync long term.

---

## Conclusion

Code quality in the touched areas is **solid for naming, layering, and documentation**, with **test duplication** and **a few stale or misleading names/docs** as the main clean-code issues. The largest concern is **incomplete end-to-end integration** (registry → worktree session), which is an architecture/requirements gap documented in the evaluation report rather than a local refactor of `worktree.rs` or `project_storage.rs` alone.
