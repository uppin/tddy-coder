# 2026-07-03 — `--append-system-prompt-file` passthrough

**Type:** Feature

`SandboxRunnerArgs` gains `append_system_prompt_file`, threaded through `SpawnClaudePtyParams` into the in-jail `claude` argv, so a daemon-hosted managed-codebase sandboxed session can inject the workflow recipe's orchestration system prompt. Feature [managed-codebase-workflow.md](../../../../docs/ft/coder/managed-codebase-workflow.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-runner, tddy-daemon)
