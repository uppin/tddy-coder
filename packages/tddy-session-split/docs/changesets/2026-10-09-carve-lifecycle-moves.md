# 2026-10-09 — New crate: split and sandboxed-codebase sessions

**Type:** Architecture

Created by `#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)) from the split topic of
`tddy-session-lifecycle`: the paired agent and codebase start, resume and teardown, `SplitSessions` and
`trait SplitHost`. 3,361 production lines in 17 modules, 37 tests. It does not depend on
`tddy-agent-launch`. Layout: [module-layout.md](../module-layout.md).
