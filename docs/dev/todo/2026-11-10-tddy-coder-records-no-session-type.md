# 2026-11-10 — Legacy tool-session records name no session type

**Category:** Deferred from `declared-session-types`
**Source:** [`../changesets/2026-11-11-declared-session-types.md`](../changesets/2026-11-11-declared-session-types.md)

Until `declared-session-types`, `tddy-coder` wrote its own `.session.yaml` with no `session_type`
(an absent one silently meant `tool`). It now records the session type its host declares for the
`tddy_coder` launch (`tddy-coder/src/run.rs`, `declared_tool_session_type`), so every new record is
explicit.

Records already on disk still name none. So that they resume,
`tddy-agent-launch::harness_from_spec::recorded_session_launch` resumes a record with **no** session
type as the `tddy_coder` launch **when it names a `tool`**, and refuses one naming neither (marked
`TODO(session-types)`). `ConnectSession`'s terminal-room check reads the same rule. The web's
`attachClaim.ts` likewise treats an empty recorded type as a tool session's
(`isLegacyToolRecord`).

**Remaining:** drop the `tool`-field rule (and the web's empty-type case) once no legacy record
needs to resume — or migrate old records by writing the declared session type into them.

**Why deferred:** the records are on operators' disks; dropping the rule now would stop every
pre-existing tool session from resuming.
