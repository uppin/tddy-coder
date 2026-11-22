# 2026-11-22 — A resume refuses a duplicate or running launch of a session's agent

**Type:** Fix

`CliSessionManager::claim_main_launch` (`tddy-cli-sessions`), taken first by the host resume `resume_cli_harness` (`tddy-agent-launch`, `opencode_cli_spawn/resume_guard.rs`): a second resume in flight or a resume of a running session is refused `FAILED_PRECONDITION`. Full record: [`docs/dev/changesets/2026-11-22-resume-main-launch-claim.md`](../../../../docs/dev/changesets/2026-11-22-resume-main-launch-claim.md) (#998).
