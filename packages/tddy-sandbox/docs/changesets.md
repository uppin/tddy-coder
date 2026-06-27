# Changesets Applied

Wrapped changeset history for tddy-sandbox.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-27** [Feature] **Darwin-sandboxed Claude CLI sessions** — new crate: `SandboxSpec`, `SandboxHandle`, `SandboxError::Unsupported`, `SandboxContextDir` (read-only context + `REMOTE_APPENDIX`), spawn facade; acceptance: `unsupported_on_non_darwin`. Feature [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md); architecture [architecture.md](./architecture.md). (tddy-sandbox, tddy-sandbox-darwin, tddy-daemon, tddy-tools, tddy-service)
