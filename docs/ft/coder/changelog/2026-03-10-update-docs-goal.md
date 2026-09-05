# 2026-03-10 — Update-Docs Goal

- **New goal**: `update-docs` runs after refactor as the final workflow step. Reads planning artifacts (PRD.md, progress.md, changeset.yaml, acceptance-tests.md, evaluation-report.md, refactoring-plan.md) and updates target repo documentation per repo guidelines.
- **Workflow**: Full chain is plan → acceptance-tests → red → green → [demo] → evaluate → validate → refactor → update-docs → end.
- **State machine**: `RefactorComplete` → `UpdatingDocs` → `DocsUpdated` (terminal).
- **CLI**: `--goal update-docs --session-dir <path>` accepted by tddy-coder and tddy-demo.
- **CursorBackend**: Supports UpdateDocs (unlike Validate/Refactor which require Agent tool).
- **Schema**: `update-docs.schema.json` with goal, summary, docs_updated.
- **Packages**: tddy-core (workflow/update_docs.rs, parse_update_docs_response, TddWorkflowHooks, tdd_graph), tddy-coder (run.rs value_parser).
