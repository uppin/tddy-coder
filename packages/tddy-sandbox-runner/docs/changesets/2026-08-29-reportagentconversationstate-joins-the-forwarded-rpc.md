# 2026-08-29 — `ReportAgentConversationState` joins the forwarded-RPC allowlist

**Type:** Feature

an agent the jail was seeded with runs its turn loop in the jail, so the facilitating daemon is never asked to open a conversation for it and its roster row would sit at UNSPECIFIED while the agent demonstrably worked. The runner now forwards that report over the `SessionChannel` alongside the roster and conversation RPCs, so the only possible source of that row's status can reach the daemon at all.
