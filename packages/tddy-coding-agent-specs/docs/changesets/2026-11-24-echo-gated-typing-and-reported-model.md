# 2026-11-24 — `coding_agent.model_report`: where a running agent reports its model

**Type:** Fix

`CodingAgentSection.model_report: Vec<ModelReportSource>` (`spec/model_report_slot.rs`, read through
`CodingAgentSpec::model_report()`): `hook_field` and `usage` sources with `events` and `ignore`;
`validate_model_report` refuses a field-less hook source and a usage source on an agent with no
`usage` binding. `cursor.yaml` reads `model` off `sessionStart`, `beforeSubmitPrompt` and `stop`;
`claude.yaml` off `SessionStart`, then the transcript on `Stop`, ignoring `<synthetic>`. Tests:
`tests/spec_model_report.rs`.

Doc: [coding-agent-specs.md § Model report](../coding-agent-specs.md#model-report-coding_agentmodel_report). Full record: [cross-package entry](../../../../docs/dev/changesets/2026-11-24-cursor-tui-submit-and-resume-model.md).
