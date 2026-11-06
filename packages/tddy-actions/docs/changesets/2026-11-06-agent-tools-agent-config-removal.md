# 2026-11-06 — The path form of a tool's home

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `HomeSection::Path { path }` (`home: { kind: path, path: <dir> }`): an absolute operator directory; `ToolDef::parse` refuses a relative one naming the file and the path.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
