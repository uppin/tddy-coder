# 2026-10-24 — Supervised clones stream progress

**Type:** Feature · `#start-progress` 6/8 ([#739](https://github.com/uppin/tddy-coder/pull/739))

`supervisor_spawn::clone_repo_via_supervisor_with_progress` replaces `clone_repo_via_supervisor_with_env`
and its 25 ms polling; `clone_request`, `clone_request_with_env`, `program_in_path` and
`resolve_git_program` are deleted (git is the supervisor's to resolve). `CloneHandle::through_supervisor`
stops a supervised clone with `StopSession`. See [clone-run.md § Supervised clones](../clone-run.md#supervised-clones).
