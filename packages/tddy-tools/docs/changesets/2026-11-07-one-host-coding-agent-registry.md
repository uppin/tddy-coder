# 2026-11-07 — `list-models --tools-dir`

**Type:** Feature

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)).

- `ListModelsArgs.tools_dir` (`--tools-dir <dir>`). The `tddy-tools` binary (`tddy-tools-cli`'s `agent_models_catalog`) loads `CodingAgentRegistry::load(dir)` — built-ins plus that directory's overrides — and refuses a directory that does not load, naming it; absent, `builtin_cached()` as before. Pinned by `tddy-tools-cli/tests/list_models_tools_dir.rs`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-one-host-coding-agent-registry.md).
