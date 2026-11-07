# 2026-11-07 — The shared package-boundary checker is not a web-affected global input

**Category:** Missing test trigger (CI gate)
**Source:** `#web-split` 3/16 (PR #840), `/validate-changes`

Every web package's boundary test (`src/packageBoundary.test.ts`) runs `scripts/web-package-boundary.ts`,
but that file is not in `GLOBAL_INPUTS` of `scripts/web-affected.ts`. A PR changing only the checker
re-runs no package check, so a regression in it would not be caught by any package's PR check (only by
its own `scripts/web-package-boundary.test.ts`).

**Why deferred:** `GLOBAL_INPUTS` belongs to #838 (`#web-split` 1/16); this node does not edit it.

**What closing it takes:** add `scripts/web-package-boundary.ts` to `GLOBAL_INPUTS` (or map it to every
package with a boundary test) with a `web-affected` unit test asserting a checker-only change selects
those packages. Natural owner: the `#web-split` sweep node.
