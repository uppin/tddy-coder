# tddy-coding-subagent

A **coding agent** — Claude Code (`claude-agent-acp`), Codex (`codex-acp`), OpenCode (`opencode acp`)
— as a session subagent the main agent opens through `tddy-tools`' `subagent_*` tools.

| Module | Role |
|---|---|
| `catalog` | `CodingAgentDef` rows from ACP harness profiles and ACP `allowed_agents` ids |
| `confinement` | which harnesses can have their native tools withdrawn in a managed session |
| `launcher` | `CodingAgentLauncher` — the port the daemon implements to start the agent as the session's OS user |
| `permissions` | `DenyAndRecord` — denies every permission request and records it for the turn's messages |
| `session` | `CodingAgentSession: SubagentSession` over `tddy_agent_backend::AcpDriver` |

PRD: [docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md](../../docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md)
