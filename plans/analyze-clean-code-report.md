# Clean-code analysis: default-free-prompt / start-slash branch

## Summary

The branch introduces a focused `feature_start_slash` module, a single public default helper in `recipe.rs`, and consistent `run.rs` wiring via `default_unspecified_workflow_recipe_cli_name()`. Overall structure is clear: resolution stays delegated to `tddy_workflow_recipes`, parsing is isolated, and CLI/docs describe the new default. Main improvement areas are **literal duplication** of the `free-prompting` CLI name, **very long public names** in `feature_start_slash`, **paired duplication** in `recipe_arc_for_args` / `validate_recipe_cli`, and **logging verbosity** at `info` for routine parse/menu paths. Test coverage is present and readable; `recipe.rs` splits tests into two modules without a strong need.

---

## What is good

- **Module documentation**: `feature_start_slash.rs` has a clear crate-level purpose, PRD pointer, and cross-links to `resolve_workflow_recipe_from_cli_name`. `recipe.rs` documents the default semantics and the resolver’s role.
- **Single source of recipe resolution**: Parsing defers to `resolve_workflow_recipe_from_cli_name`; `recipe.rs` re-exports and wraps consistently. No parallel name lists in `parse_feature_start_slash_line`.
- **API shape for parsing**: `Option<Result<...>>` is documented (`None` = not a start line; `Some(Ok/Err)` = parsed). Error messages align with the shared resolver.
- **`run.rs` wiring**: Default is applied in one conceptual place (`default_unspecified_workflow_recipe_cli_name`) for `recipe_arc_for_args`, validation, changeset init, goal clearing, and agent-from-changeset. Field docs on `Args` / clap structs mention `free-prompting` as default.
- **Exports (`lib.rs`)**: `feature_start_slash` items are grouped in one `pub use` block; naming matches the submodule, which aids discoverability.
- **Function size**: Helpers in `feature_start_slash.rs` and `recipe.rs` are small and linear; no oversized blocks in the reviewed paths.
- **Tests**: `feature_start_slash` tests cover happy path, menu coverage, post-completion name, and unknown suffix behavior. `recipe.rs` asserts default and resolver parity.

---

## Issues / smells

1. **Magic string `free-prompting`**  
   Appears in `default_unspecified_workflow_recipe_cli_name`, `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete`, tests, and implicitly via behavior. Drift risk if the canonical CLI name ever changes.

2. **Extremely long public name**  
   `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete` is accurate but hard to read at call sites and in `pub use` lists. Shorter names with module prefix (`feature_start_slash::...`) already disambiguate.

3. **Constant-as-function**  
   `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete` only returns `"free-prompting"` with logs. A `pub const` plus one doc line, or reusing the same helper as `default_unspecified_workflow_recipe_cli_name` (if semantics match), would reduce noise—unless a distinct symbol is required for call-site clarity.

4. **Duplication in `run.rs`**  
   `recipe_arc_for_args` and `validate_recipe_cli` repeat the same `args.recipe.as_deref().unwrap_or_else(|| crate::default_unspecified_workflow_recipe_cli_name())` pattern.

5. **Logging level**  
   `parse_feature_start_slash_line` and `feature_slash_menu_start_command_labels` emit `log::info!` for normal outcomes (resolved line, label counts). That may be noisy compared to typical “info = user-visible milestones” usage.

6. **Test module split in `recipe.rs`**  
   `mod tests` and `mod default_recipe_tests` both hold small unit tests; merging keeps one convention (`tests` only) unless the split documents a reason (e.g. integration vs unit).

7. **Naming consistency across crates**  
   `default_unspecified_workflow_recipe_cli_name` (tddy-coder) vs `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete` (workflow-recipes) describe related “default to free-prompting” ideas with different vocabularies (`unspecified` vs `after_start_slash_structured_workflow_complete`).

---

## Suggested refactors (concrete, small)

1. **Introduce one canonical `&'static str` for the CLI name** (e.g. in `tddy_workflow_recipes::recipe_resolve` or `free_prompting` module, or next to `FreePromptingRecipe`), and reference it from `default_unspecified_workflow_recipe_cli_name`, `next_session_*`, and tests. Keeps a single edit point.

2. **Rename** `next_session_recipe_cli_name_after_start_slash_structured_workflow_complete` to something like `post_start_slash_workflow_complete_recipe_cli_name` (or `default_recipe_cli_name_after_start_slash_session`) and keep the old name as a deprecated alias only if external API stability demands it.

3. **Add a private helper in `run.rs`**, e.g. `fn effective_recipe_cli_name(args: &Args) -> &str`, used by both `recipe_arc_for_args` and `validate_recipe_cli` (and optionally other call sites that repeat the same `unwrap_or_else`).

4. **Demote routine parse/menu logs** from `info` to `debug` (or gate verbose lines behind `log::log_enabled!`) so successful parses do not flood logs.

5. **Merge `default_recipe_tests` into `tests`** in `recipe.rs` and drop the extra module, unless you plan to grow default-recipe-only tests significantly.

6. **If post-completion default must equal “unspecified CLI default”**, consider having `next_session_*` call or alias `default_unspecified_workflow_recipe_cli_name` from `tddy-coder` is wrong (wrong crate direction); instead define shared const in `tddy_workflow_recipes` and use it from both crates to avoid semantic duplication without inverted dependencies.

---

*Generated for branch review: `feature_start_slash.rs`, `recipe.rs`, `run.rs` default wiring, `lib.rs` exports.*
