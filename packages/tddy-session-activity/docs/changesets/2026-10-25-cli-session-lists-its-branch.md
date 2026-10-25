# 2026-10-25 — A CLI harness session lists the branch its changeset records

**Type:** Bug fix

[#748](https://github.com/uppin/tddy-coder/pull/748). `session_list_status_from_session_dir` answered
CLI harness sessions (claude-cli, cursor-cli, …) from `.session.yaml` alone, on the premise that they
have no changeset, and listed `branch` empty. A start that creates a worktree writes the branch it
created to `changeset.yaml`, so every such session listed with `branch: ""` — in `ListSessions` and
in the project-home `ListProjectSessions` tool. The harness path now reads `branch` from the changeset
when there is one; a CLI session without a changeset still lists an empty branch.

`orchestrator_session_id` stays empty for CLI sessions: a session a project home starts belongs in
the project's drawer group beside the home session, not nested under it.
