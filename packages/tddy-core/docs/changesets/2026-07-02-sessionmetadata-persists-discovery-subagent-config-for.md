# 2026-07-02 — `SessionMetadata` persists discovery-subagent config for resume

**Type:** Feature

`discovery_subagent`/`fastcontext_url`/`fastcontext_model`/`fastcontext_max_turns`/`subagent_replaces` (all optional, absent for legacy files) so a resumed sandboxed daemon session reconstructs the same subagent configuration it started with. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core, tddy-daemon)
