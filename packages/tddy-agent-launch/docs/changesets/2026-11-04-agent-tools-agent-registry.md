# 2026-11-04 — One coding-agent registry

**Type:** Feature

`#agent-tools` 12/19 ([#801](https://github.com/uppin/tddy-coder/pull/801)).

- The CLI launch path is read from the registry (`cli_launcher`); `split_host` and `requires_model` come from the spec; a YAML-only agent starts as `<id>-cli`; an ssh session's hook slot files are written on the ssh target.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-04-agent-tools-agent-registry.md).
