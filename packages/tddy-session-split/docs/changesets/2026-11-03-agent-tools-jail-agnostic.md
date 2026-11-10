# 2026-11-03 — The split host agent runs from claude.yaml

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)).

- `split_claude_extra_args` and `tddy_tools_only_mcp_args` render from the embedded `claude.yaml`. They use its `host_agent` variant and slots, which adds `--setting-sources user` and withdraws `LS`.
- `NATIVE_FILESYSTEM_TOOLS` is pinned equal to the variant by a test.

See [module-layout.md](../module-layout.md).
