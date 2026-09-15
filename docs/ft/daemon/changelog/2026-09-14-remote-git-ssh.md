# 2026-09-14 — Serve remote-git packs through SSH sessions

- `RemoteGitService.Serve` spawns pack verbs on an SSH Host alias when the project's OS user has a
  session with `ssh_config_host` set; otherwise spawn stays on the local `main_repo_path`.
- `tddy-remote-git-repo` and the wire proto are unchanged.
