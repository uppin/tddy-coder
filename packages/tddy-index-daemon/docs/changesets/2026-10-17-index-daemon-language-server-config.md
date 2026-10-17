# 2026-10-17 — `--rust-analyzer` names the language server

**Type:** Feature

`--rust-analyzer <PATH>` names the program the Rust `LspAllowList` entry launches; without it, `rust-analyzer` on the
process's `PATH`. A program that is not there is refused as `FailedPrecondition: language server not found: …`.
Pinned by `dual_transport_acceptance::runs_the_language_server_named_by_rust_analyzer_and_refuses_one_that_is_not_there`.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-17-index-daemon-language-server-config.md).
