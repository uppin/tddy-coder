# 2026-11-22 — A session owns its work queue

A work queue belongs to a session and is created when the session starts — with "with work queue", or
for any orchestrator. Its owner works it with every work tool; a session one of its items links to reads
it; no role grants access. Addressing a session without a queue says how one comes to exist. Stacks kept
as project records before this release are removed on upgrade. See
[PR stacking § Where a stack lives](../pr-stacking.md#where-a-stack-lives) (`#workqueue` 5/14, PR #1006).
