# Remove line-budget tests; tddy-daemon's wiring-only rule moves to its AGENTS.md

**Date:** 2026-10-26 · **Type:** Chore · **Packages:** tddy-daemon, tddy-core, tddy-daemon-rpc,
tddy-daemon-kernel, tddy-github, tddy-presenter, tddy-workflow-recipes, tddy-code-restructuring,
tddy-telegram-control (docs)

The developer removed every test that capped a crate's or a file's line count. A size cap tripped on
unrelated feature work (`#start-parity` 5/9 pushed `tddy-daemon` to 6,521 against 6,500) and only
measured size, not whether the code belonged.

**Removed (line caps only):** `unbundle_endpoint::the_daemon_is_wiring_and_nothing_more`;
`core_facade_shape`'s 10k-per-crate cap and `tddy_core_is_only_a_wiring_point`'s 200-line half;
`rpc_handlers_shape::every_crate_receiving_code_stays_within_10k_production_lines`;
`core_foundations_shape`'s 400-per-module cap and `the_changeset_parent_is_a_facade`'s 200-line half;
`telegram_extraction_shape`'s 800-per-module cap; `git_plumbing_shape`'s `worktree.rs` budget;
`presenter_split_shape`'s parent budget and 500-per-partition cap; `module_shape`'s 500-per-module cap;
`tddy-code-restructuring/tests/engine_file_budget_shape.rs` (the whole file).

**Kept:** every dependency-edge, module-set, visibility and field-count assertion. Where a removed test
also asserted structure, that half stays as its own test: `tddy_core_is_only_a_wiring_point` (defines
nothing), `every_crate_receiving_code_holds_production_code`, `the_changeset_parent_publishes_rather_than_defines`,
`the_control_module_is_split`, `the_parent_keeps_the_struct`.

**Added:** `packages/tddy-daemon/AGENTS.md` (with a `CLAUDE.md` link) — no non-wiring functionality may
be added to `tddy-daemon`.

The repo's 500-production-line file budget remains a review convention (`pr-wrap`, code-issue records);
no test enforces it. Docs that named the removed tests are updated: `tddy-telegram-control/docs/architecture.md`,
`tddy-code-restructuring/README.md`, and three `docs/dev/todo/` entries.
