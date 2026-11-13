# 2026-11-10 — The `effort` slot

**Type:** Feature

- `CodingAgentSection.effort: Option<EffortSlot { levels, default, carry: EffortCarry::{Flag | Env} }>`, YAML `effort: { levels, default?, flag | env }`; `CodingAgentSpec::effort()`. Both or neither carry is refused as the slot is read; `validate_effort_slot` (from `parse_section`) refuses empty, blank or repeated levels and a `default` outside them.
- `claude.yaml` declares `levels: [low, medium, high, xhigh, max]`, `flag: --effort`.
- Docs: [coding-agent specs § Effort](../coding-agent-specs.md#effort-coding_agenteffort).

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md).
