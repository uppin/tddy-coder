# tddy-coding-subagent

A **coding agent** — Claude Code (`claude-agent-acp`), Codex (`codex-acp`), OpenCode (`opencode acp`)
— as a session subagent the main agent opens through `tddy-tools`' `subagent_*` tools.

| Module | Role |
|---|---|
| `catalog` | `CodingAgentDef` rows from ACP harness profiles and ACP `allowed_agents` ids |
| `confinement` | which harnesses can have their native tools withdrawn in a managed session |
| `launcher` | `CodingAgentLauncher` — the port the daemon implements to start the agent as the session's OS user |
| `permissions` | `DenyAndRecord` — denies every permission request and records it for the turn's messages |
| `parked_permissions` | `ParkedPermissions` — the handler a conversation installs: parks each request until the caller's answer resolves it |
| `session` | `CodingAgentSession: SubagentSession` over `tddy_agent_backend::AcpDriver` |

Architecture: [docs/architecture.md](docs/architecture.md). Product: [Session agent roster § Coding agents](../../docs/ft/daemon/session-agent-roster.md#coding-agents).
