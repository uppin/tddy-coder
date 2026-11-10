# 2026-11-02 — claude.yaml declares its jail

**Type:** Feature

`claude.yaml` gains `home: { kind: persistent, key: claude }`, the credential seed and install mirrors, `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` overlays, `CLAUDE_CODE_OAUTH_TOKEN` (`host-env`, `when: unrouted`), `policy:` and `network.hosts` with `oauth_inbound: true`. Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
