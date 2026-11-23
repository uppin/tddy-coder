# 2026-11-22 — The pr-stack recipe's removal (`#workqueue` 1/14, #1002)

**Type:** Feature (removal)

`StackChildSpawnHandler`, `stack_seed_validation` and the start request's seed gate are deleted; `managed_spawn_handler` binds only the grill-me conversation spawner. A session recorded with the removed `pr-stack` recipe (or an alias) is refused on resume with the recipe named (`recorded_managed_recipe`). Each remote liveness probe logs its bound and answer at debug level. Cross-package: [2026-11-22-remove-pr-stack-recipe.md](../../../../docs/dev/changesets/2026-11-22-remove-pr-stack-recipe.md).
