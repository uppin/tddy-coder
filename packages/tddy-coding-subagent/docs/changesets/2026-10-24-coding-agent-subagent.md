# 2026-10-24 — A coding agent as a session subagent
**Type:** Feature

New crate. `CodingAgentSession` drives one `AcpDriver` per conversation; `CodingAgentDef` rows come
from ACP harness profiles and `allowed_agents`; `HarnessConfinement` (`confinement_for`,
`refuse_unless_confinable`) decides which harnesses a managed session may run; `DenyAndRecord`
denies every permission request; `CodingAgentLauncher` is the port the daemon implements. See
[architecture.md](../architecture.md) (#721, `#coagents` 2/8).
