# 2026-11-24 — Cursor's start-option model list lacks models its TUI offers

**Category:** Future enhancement (upstream)
**Source:** `fix/cursor-tui-submit-and-resume-model` ([changeset](../1-WIP/2026-11-24-cursor-tui-submit-and-resume-model.md)).

`DescribeSessionStartOptions` lists a Cursor session's models from `agent --list-models` (the
`cursor-models` adapter, `cursor.yaml` `models.list_command`). On 2026-11-24 (Cursor Agent
2026.11.22) that list had GLM 5.2 but not GLM 5.3 or GLM 5.3 Flash, which the TUI's `/model` picker
offers.

The gap is Cursor's, not ours: `agent --list-models` and `agent models` print the same 139 models,
neither naming GLM 5.3; only the in-TUI picker does. `agent --model glm-5p3-flash-high` is accepted
all the same, so a model missing from the list still starts and resumes.

**Why deferred:** no CLI command exposes the picker's list; scraping the TUI is not a fix worth
owning. Revisit when Cursor's `models` output catches up, or if it grows a machine-readable catalogue.
A session already on such a model keeps it across a resume (`coding_agent.model_report`).
