# 2026-03-28 — Connection screen: host selection + session workflow status (TUI parity)

- **ConnectionService**: **`ListEligibleDaemons`** lists daemon instances for the Host dropdown; **`StartSession`** and **`SessionEntry`** carry **`daemon_instance_id`**, and **`ListSessions`** includes workflow columns (**`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, **`model`**).
- **ConnectionScreen**: Per-project **Host** dropdown (populated after auth), **Host** column on project and **Other sessions** tables, **`daemonInstanceId`** on **`StartSession`**, plus **Goal**, **Workflow**, **Elapsed**, **Agent**, and **Model** columns via **`SessionWorkflowStatusCells`**. Tables use horizontal scroll when needed. Session list polling uses a **2** second interval when any session is active (**`isActive`**), and **5** seconds otherwise; projects still refresh every **5** seconds. Cypress covers host selection and workflow column display.
- **Deferred**: LiveKit common-room peer discovery and cross-daemon spawn routing remain future work; the daemon rejects non-local **`daemon_instance_id`** until that ships.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Daemon mode: Connection screen).
- **Elapsed semantics (QA)**: [web-terminal.md](../web-terminal.md) (Daemon mode: Connection screen — **TUI vs web elapsed**).
