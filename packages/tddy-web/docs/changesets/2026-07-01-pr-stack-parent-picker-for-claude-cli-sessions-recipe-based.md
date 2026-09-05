# 2026-07-01 — PR stack parent picker for Claude CLI sessions + recipe-based filter

**Type:** Feature

`CreateSessionPane` moves parent picker outside session-type branches so it renders for both tool and claude-cli sessions; filter updated from `!orchestratorSessionId` to `prStackOrchestrators()` (uses `recipe` field to identify real PR-stack orchestrators, including childless ones); `stackParent` threaded into claude-cli `startSession` call. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-service, tddy-daemon, tddy-web)
