# 2026-10-17 — The index daemon is started with the configured rust-analyzer and environment

**Type:** Feature

`IndexDaemonSpawn` carries `rust_analyzer` and `env` from `index_daemon:`; `IndexDaemonBody` passes
`--rust-analyzer <path>` after `--grpc-uds <socket>` and sets `env` on the child. Pinned in
`index_daemon_lifecycle_acceptance` (`tells_the_index_daemon_which_language_server_to_run_and_with_what_environment`,
`reads_the_language_server_and_its_environment_from_the_index_daemon_section`).

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-17-index-daemon-language-server-config.md).
