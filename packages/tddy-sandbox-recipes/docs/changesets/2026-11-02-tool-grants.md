# 2026-11-02 — `tool_grants`: lower resolved tools into jail grants

`#agent-tools` 1/12 ([#790](https://github.com/uppin/tddy-coder/pull/790)). `lower_tools(tools, jail_bin_dir, libraries)` → `ToolGrants { reads, symlinks, path_prefix }`: a literal exec read per binary, its `otool -L` library dirs plus their nearest canonical ancestors (`BinaryLibraries` seam, `HostBinaryLibraries` in production), each tool's `reads`, and `bin/<command>` links. Depends on `tddy-actions`. See [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-tool-defs.md).
