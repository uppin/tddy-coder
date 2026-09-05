# 2026-03-28 — Multi-host daemon selection (phase 1)

**Type:** Feature

Proto + RPC: **`ListEligibleDaemons`**, **`daemon_instance_id`** on **`StartSession`** / **`SessionEntry`**. Daemon: **`EligibleDaemonSource`** wiring, session listing field, local vs non-local **`StartSession`**. Web: Host dropdown, Host column, Cypress. LiveKit peer discovery and cross-daemon spawn deferred. Feature docs: `docs/ft/web/web-terminal.md`, `docs/ft/web/changelog.md`; package: `packages/tddy-daemon/docs/connection-service.md`, `packages/tddy-service/docs/changesets.md`, `packages/tddy-daemon/docs/changesets.md`. (tddy-service, tddy-daemon, tddy-web)
