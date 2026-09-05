# 2026-06-16 — tddy-build subcommands

**Type:** Feature

`build_cli.rs`: `build` / `build-list` run `tddy-build` locally or relay over `TDDY_SOCKET`; wired into `main.rs`; new dep on `tddy-build`. Also unhangs `remote_cli_acceptance` (multi-thread runtime + proper mock relay). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools, tddy-build)
