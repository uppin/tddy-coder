# 2026-04-04 — `SessionMetadata.pending_elicitation`

**Type:** Feature

Boolean in **`.session.yaml`** (serde default **`false`**); daemon **`ListSessions`** surfaces it on **`SessionEntry`**; Connection screen badge when **`true`**. Tool processes persist the flag when sessions block on human input. Feature docs: [web-terminal.md](../../../../../docs/ft/web/web-terminal.md), [daemon changelog](../../../../../docs/ft/daemon/changelog/). (tddy-core, tddy-daemon, tddy-service, tddy-web)
