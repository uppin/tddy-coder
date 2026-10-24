# 2026-10-24 — A start that clones shows git's progress, and a clone never lands half-written

A session start that has to clone its project reports a **clone** step before the worktree step, with
git's own progress — the stage, percent, object counts, bytes received and throughput. Clones have no
deadline: a large repository on a slow network finishes instead of timing out, for session starts as well
as for creating a project or adding it to a host. A clone that fails or is stopped leaves nothing behind,
so the next start never mistakes a half-written checkout for the project. On a host run by
`tddy-supervisor` the clone step shows without progress for now. See
[Project concept § Cloning at session start](../project-concept.md#cloning-at-session-start).
