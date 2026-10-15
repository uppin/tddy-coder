# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`SetProjectAccounts` carries `local_gh` both ways; `RepointPlannedPr` of a Local gh session reads the token from `gh` as the session's OS user; `GetHostTooling` passes `github_hostname`. See [the documentation](../architecture.md#repointplannedpr-and-the-github-token).
