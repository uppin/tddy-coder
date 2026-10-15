# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`DaemonSessionHost::with_github_cli`; a start or resume takes the source from the request, the session's recorded `github-source.yaml`, or this host's row, in that order. Known limit: child sessions read the host's row. See [the documentation](../session-identity.md#which-source-a-session-uses).
