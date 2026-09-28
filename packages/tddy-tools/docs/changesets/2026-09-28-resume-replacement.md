# 2026-09-28 — subagent_resume accepts a replacement call and result

**Type:** Feature

`#subagent-control` 5/5, [#557](https://github.com/uppin/tddy-coder/pull/557). Cross-package entry:
`docs/dev/changesets/2026-09-28-resume-replacement.md`.

The `subagent_resume` MCP schema gains **`replacement`** — a `{tool, arguments, result}` the
caller substitutes for the call that yielded the conversation back — parsed and validated at the
server before the request reaches the daemon.
