# 2026-10-17 — `index_daemon:` names rust-analyzer and its environment

**Type:** Feature

`IndexDaemonConfig` gains `rust_analyzer_path: Option<PathBuf>` and `env: BTreeMap<String, String>`, both optional and
omitted from serialization when unset. `config.rs`: 1,542 → 1,557 production lines.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-17-index-daemon-language-server-config.md).
