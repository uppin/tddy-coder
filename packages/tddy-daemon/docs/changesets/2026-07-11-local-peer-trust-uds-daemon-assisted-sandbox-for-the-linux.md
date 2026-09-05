# 2026-07-11 — Local peer-trust UDS + daemon-assisted sandbox for the Linux app

**Type:** Feature

a UDS `tonic` server serves `ConnectionService` via a hand-written adapter over the existing impl (shared via codegen `from_arc`); `MintLocalToken` mints a session token from the SO_PEERCRED peer uid (`local_token_login_for_uid`); `start_sandboxed_claude_cli_session` honors client `repo_path` (used directly, never auto-removed — `.worktrees` guard) + `claude_args`. Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#291](https://github.com/uppin/tddy-coder/pull/291) (draft; **not run end-to-end**; stacks on #290). (tddy-daemon, tddy-service, tddy-sandbox-app)
