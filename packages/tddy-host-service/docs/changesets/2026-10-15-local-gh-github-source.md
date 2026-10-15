# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

New `GithubCli` (`github_cli.rs`): `logged_in_user` (`gh api user --hostname H`) and `auth_token` (`gh auth token --hostname H`) as an OS user, with `GhUser` and `GithubCliRefusal`. `HostToolingProbe::probe` takes an optional GitHub host and runs `gh auth status --hostname H`; `GetHostTooling` forwards `github_hostname`. See [the documentation](../host-tooling-probe.md#the-github-cli).
