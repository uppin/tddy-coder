# 2026-11-22 — A target child's reports, colours and attachments; following a session; answered questions

A child on a Coder or SSH target now reaches its orchestrator whatever its login shell: its report
command runs under `sh`, where zsh used to empty it. `ReadSessionMessages`' `lost` counts only the
reader's own lost messages. `ReadSessionActivity` pages by each call's place in the activity log, so
a poll returns what happened since — including calls that finished since — and every call keeps its
start time. `GetSessionStart` keeps a target start's branch once it succeeds. An agent on a target
draws in colour. A session's attachments are named in its first prompt by a path its agent can read,
copied into the worktree on a target. An orchestrator's answer to an `AskUserQuestion` reaches Claude
even after the first wait has passed. See [project concept § Reporting to the orchestrator](../project-concept.md#reporting-to-the-orchestrator)
and [§ Following a session](../project-concept.md#following-a-session).
