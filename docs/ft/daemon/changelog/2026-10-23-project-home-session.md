# 2026-10-23 — A read-only project-home session per project

`EnsureProjectHomeSession` starts, resumes or returns a project's **project-home session**: a
managed session running the project's home agent on a dedicated home worktree detached at the
project's main branch ref, fast-forwarded before each resume. The daemon refuses every exec tool but
`Read`, `Grep`, `Glob`, `ReadLints` and `SemanticSearch` for it. A project with no home agent, or one
whose harness cannot enforce managed-codebase mode, is refused. A harness profile's
`provider_account` may be omitted. See
[project concept § Project-home session](../project-concept.md#project-home-session).
