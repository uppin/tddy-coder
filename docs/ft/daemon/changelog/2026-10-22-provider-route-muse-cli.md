# 2026-10-22 — Muse Code terminal sessions

A fourth CLI session type, **Muse Code** (`muse-cli`): Meta's Muse Code TUI in a PTY in a daemon-cut
worktree, resumable with `muse --continue`, offered a curated Muse model list (Muse Spark 1.3 by
default, Muse Wonder 1.0), its binary configurable under `cli_harnesses.muse-cli.binary_path`. On a
Meta provider account — and only a Meta one — it is handed the account's key in `META_API_KEY`; a
person can make a Meta account their default for Muse Code from its row on the Accounts screen.
Sandboxed and managed-codebase starts are refused. See [Muse Code CLI session](../muse-cli-session.md).
