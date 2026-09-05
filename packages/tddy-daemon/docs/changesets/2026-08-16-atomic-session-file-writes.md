# 2026-08-16 — **atomic session-file writes

**Type:** Bug Fix

daemon state goes through `write_atomic`** — `project_storage.rs`, `worktrees.rs`, `telegram_github_link.rs`, `connection_service.rs` and `cursor_cli_spawn.rs` replace `projects.yaml` and session state by swap-file-plus-rename, so a full disk leaves the registry readable instead of a 0-byte file that reads as an empty project list. Still truncating in place and worth a mode-aware `write_atomic` variant: the secret stores `github_token_store.rs`, `vnc_vault.rs`, `screen_sharing_vault.rs`, which are correct about `0600` on creation. [tddy-core architecture.md § Atomic file writes](../../../tddy-core/docs/architecture.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
