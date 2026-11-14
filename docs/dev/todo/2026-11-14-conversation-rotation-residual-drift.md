# A claude-cli session can still drift off its recorded conversation after the `/clear` rotation

**Recorded:** 2026-11-14, against `#orchestrators 6/11` (PR #883), from the investigation of a
project-home session whose resume came back on its original conversation after `/clear`.

## Background: the incident, which #883 fixes

On master a `claude-cli` session's conversation id is always its daemon session id. Every resume binds
`meta.session_id` (`tddy-agent-launch/src/cli_agent_steps/conversation.rs`), and `session-hook` drops
the hook's `session_id`/`source`. After a `/clear`, the next stop and resume therefore reopen the
session's **first** conversation.

The incident was session `01a16dc6-…` on 2026-11-14. Times are local, from the daemon log:

- 07:26:06 resume.
- 07:26:12 `/clear` starts `2dfa902e`. `ReportSessionStatus` for SessionEnd and SessionStart reaches
  the daemon at 07:26:12.636 and 07:26:13.077.
- 07:42:46 the idle reaper stops the session.
- 07:46:54 the resume argv is `--resume 01a16dc6-…`, so `2dfa902e` is lost.

Both hook reports arrived in order and well inside the hook timeout, so #883's
`SessionEnd(clear)` → `SessionStart(clear)` rotation would have recorded `2dfa902e`. Ordinary
`claude-cli` sessions are affected identically. Project-home sessions just hit it more often because
of the idle reaper.

## What is left open

#883 deliberately rotates **only** on an armed `/clear`. For example,
`conversation_rotation_record.rs::a_session_start_that_is_not_a_clear_rotates_nothing` and the
agent-tab-clear tests pin that. It does this so a foreign `SessionStart` cannot hijack the record:
an agent tab, or a `claude -p` started in the worktree, inherits `settings.local.json`. Those defences
are right. They leave these cases:

1. **In-CLI `/resume <other>`.** Claude fires `SessionStart(source=resume)` with a different id and
   no `SessionEnd(reason=clear)`, so nothing arms. The session silently resumes the previous recorded
   id on its next start.
2. **A lost arm.** `session-hook` fails quietly. If the `SessionEnd(clear)` report is dropped (daemon
   restarting, socket error, hook timeout under load), the following `SessionStart(clear)` is ignored.
   Nothing reconciles the record later.
3. **No existence check on resume.** `resumed_conversation` binds the newest `agent_conversations` id
   even when `{agent_home}/.claude/projects/*/<id>.jsonl` does not exist. The failure is then whatever
   Claude does with an unknown `--resume` id, and the daemon never says why.
4. **Stale comments.** `tddy-agent-launch/src/project_home_idle.rs` (module doc, and the comment at
   `stop_session`) says "Nothing in `.session.yaml` changes, so the next ensure resumes the same
   conversation". With #883, `.session.yaml` *does* change on `/clear`, and that change is what makes
   the claim true.
5. **No test of the whole chain.** The rotation suites (`session_hook_conversation_rotation`,
   `conversation_rotation_record`, `spec_launch_conversation_rotation`) each test one piece. None drives
   a real hook report, then a stop through the idle reaper (`reap_idle_project_home_sessions`), then
   `EnsureProjectHomeSession`, and asserts the respawned argv carries `--resume <post-clear id>`.

## Developer decision

None yet. Rejected during the investigation: recording "the newest transcript in the project dir" at
stop or idle-stop. It guesses from mtimes, and the project directory is shared by every `claude` run in
that cwd, which is the same foreign-run ambiguity #883's arm exists to prevent.

## What would close it

- (1) A second rotation rule in `claude.yaml`: `SessionStart(source=resume)`, armed by the
  `SessionEnd` that names the current id. First confirm which `reason` Claude sends for an in-CLI
  `/resume`. Keep the agent-tab and foreign-run guards.
- (2) Decide with the developer whether a `SessionStart` whose id differs from current, and whose
  `transcript_path` lies in the session's own agent-home project dir, may rotate without an arm. That
  is a fallback, so it needs explicit consent.
- (3) A refusal, or a loud log naming the session and the id, when the resumed id has no transcript.
  Do not fall back without consent.
- (4) Reword the idle-reaper comments.
- (5) One acceptance test of the chain described in item 5.

## Where

- `packages/tddy-agent-launch/src/cli_agent_steps/conversation.rs`: `current_conversation_id`,
  `resumed_conversation`
- `packages/tddy-session-activity/src/service/conversation_rotation.rs`: `follow_conversation_rotation`
- `packages/tddy-coding-agent-specs/src/slots/conversation.rs`: `RotationSlot::step`
- `packages/tddy-coding-agent-specs/specs/claude.yaml`: `coding_agent.conversation.rotation`
- `packages/tddy-agent-launch/src/project_home_idle.rs`: module doc and the `stop_session` comment
