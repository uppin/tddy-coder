# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

New `local_gh` module: `github_hostname_of(git_url)` (https, scp-style and `ssh://`; the host must be a DNS name or bracketed IPv6 literal, else `LocalGhError::UnreadableGitUrl`) and `local_gh_git_identity(login, id)` (the noreply identity). See [the documentation](../github-identity-resolution.md#local-gh).
