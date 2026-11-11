# 2026-11-10 — tddy-spawn: a workflow spawn records its usage context

`plan_session_child` writes `usage-context.json` (the agent, the OS user's home, the repo, the
model) when it is given an agent; a spawn without one records nothing
([todo](../../../../docs/dev/todo/2026-11-11-workflow-spawn-without-agent-records-no-usage-context.md)).
`#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891).
