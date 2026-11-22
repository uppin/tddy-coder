# 2026-11-22 — The pr-stack recipe's removal (`#workqueue` 1/14, #1002)

**Type:** Feature (removal)

A Coder one-shot's watchdog closes fd 3, so a watchdog that outlives its command holds none of the command's output; each one-shot logs its client-exit and pipe-end timings at debug level. Cross-package: [2026-11-22-remove-pr-stack-recipe.md](../../../../docs/dev/changesets/2026-11-22-remove-pr-stack-recipe.md).
