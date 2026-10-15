# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`GITHUB_SOURCE_ENV` (`TDDY_GITHUB_SOURCE`), `local_gh_source_value`, `parse_github_source`, `SessionTokenSource`, `request_github_token_from`: a Local gh session reads its token from `gh` in its own process and never asks the socket; a malformed value is an error. See [the documentation](../architecture.md#the-github_source_env-variable-and-local-gh).
