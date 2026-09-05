# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Feature

install acceptance covers `tddy-remote-git-repo`: the fixtures write it as a release artifact (the installer fails fast on a missing one), the exact installed-set assertion is extended rather than relaxed, and two new tests pin the modes that matter — `--user` landing it on `~/.local/bin` (the developer machine, where git execs it) and a system install landing it in `BIN_DIR` at `0755`. (tddy-e2e)
