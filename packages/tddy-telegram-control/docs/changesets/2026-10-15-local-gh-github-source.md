# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`telegram_git_environment(project, os_user, &GithubCli)`: a Telegram start of a Local gh project carries the host's `gh` identity, and `account_identity_unavailable_notice` returns `None` for it. Known limit: Telegram builds its own `GithubCli`. See [the documentation](../architecture.md#github-account-of-a-telegram-started-session).
