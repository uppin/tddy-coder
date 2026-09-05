# 2026-03-29 — Activity log: user prompts and incremental agent streaming

- **Core**: **`presenter::activity_prompt_log`** (**`User: `** / **`Queued: `** prefixes) wires **`SubmitFeatureInput`** and **`QueuePrompt`** into **`activity_log`** and **`ActivityLogged`**. **`presenter::agent_activity`** holds incremental tail helpers and channel policy constants. **`Presenter::poll_workflow`** on **`WorkflowEvent::AgentOutput`** maintains a growing partial **`AgentOutput`** row in **`activity_log`**, finalizes completed lines at newline boundaries, and broadcasts each chunk via **`PresenterEvent::AgentOutput`** without duplicating routine streaming text on **`ActivityLogged`**.
- **Tests**: Presenter unit tests in **`tddy-core`**; **`presenter_integration`** acceptance tests for user and queued prompt lines.
- **Docs**: [Activity log streaming](../activity-log-streaming.md), [overview](../1-OVERVIEW.md), **`packages/tddy-core/docs/architecture.md`**.
