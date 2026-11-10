# 2026-11-06 — The spec schema carries everything the deleted descriptors held

**Type:** Refactor (schema completion)

`#agent-tools` 20/28 ([#828](https://github.com/uppin/tddy-coder/pull/828)).

- `CodingAgentSpec::argv_params()` (the params `run.argv` binds), `binds_argv_param(param)`,
  `render_run_argv(...)` and `model_listing_argv()` — the argv templates are the CLI-arg schema,
  so no descriptor field survives beside them.
- `curated_models()` answers `(id, label)` pairs plus the default; `CuratedModel` is a bare id
  or `{ id, label }` in YAML.
- `CodingAgentSection` gains `permission_modes` and `transcript_usage`;
  `CodingAgentRegistry::builtin_cached()` is the process-wide builtin view;
  `Capability::name()` names a capability for the wire.
- Every consumer that read the legacy `CliHarness` table, the curated-model lists or the
  context-glob table now reads these; see
  [coding-agent-specs.md § The only home of agent metadata](../coding-agent-specs.md#the-only-home-of-agent-metadata-the-sweep-agent-tools-2028)
  and the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-sweep-yaml-only.md).

Code issue: `oversized-file-spec` regressed 815 → 981 production lines on the new schema; the
split remains to be scheduled.
