# 2026-11-21 — An orchestrator reads every tddy worktree, agent home and chat file on its host

An orchestrator session's `Read`, `Grep` and `Glob` accept absolute paths and answer for any path in
its reach: every worktree of every tddy project on its host, the home and chat directories each coding
agent declares (`coding_agent.home_dirs`), and the paths granted to the session. In another worktree
`.git` and git-ignored files stay hidden. A path outside reach is refused, naming `RequestPathAccess`.
Other sessions' reads are unchanged. See
[project concept § Read reach](../project-concept.md#read-reach). (`#orchestra` 1/9, PR #979)
