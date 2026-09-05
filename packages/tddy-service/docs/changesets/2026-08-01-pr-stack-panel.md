# 2026-08-01 — **PR-stack panel

**Type:** Feature

base-sync, reorder and pull-base wire surface** — additive only, no renumbering: `QueryBranchRequest.base_branch = 4` (empty means the caller could not name a base, and the leg reports itself unavailable rather than substituting a default — unlike `RepointPlannedPr`, because this is a display whose number must describe the base the row itself shows); `BranchResolution.base_sync = 6` → new `BranchBaseSync` (behind/ahead counts, conflict flag with paths, the refs actually compared, and an explicit `unavailable` discriminator so a comparison that could not be made is never read as clean); `BranchWorktree.dirty = 3` / `dirty_paths = 4`; `rpc ReorderPlannedPr` returning `stack_plan_json` (the shape `AddPlannedPr`/`RepointPlannedPr` already return, so the web reuses one parser); `rpc PullBaseIntoBranch` returning a fresh `BranchResolution` plus `strategy`/`changed`/`pushed`/`push_error`. (tddy-service)
