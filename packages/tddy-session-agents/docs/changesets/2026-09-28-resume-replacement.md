# 2026-09-28 — The resume RPC carries the caller's replacement

**Type:** Feature

`#subagent-control` 5/5, [#557](https://github.com/uppin/tddy-coder/pull/557). Cross-package entry:
`docs/dev/changesets/2026-09-28-resume-replacement.md`.

`ResumeAgentConversationRequest` gains `replacement_json` — parsed in `turn_request()`, composed
with the yield-conditions parse, refused with `Status::invalid_argument` before any turn is
stamped when it is not a replacement this build can run with — and the resume framing appends the
call and result server-side, exactly as the local path does.

Code issue at wrap: `docs/code-issues/oversized-file-service.md` — 1,149 production lines (was
1,122), +27. Open, unclaimed; restructuring deferred past the `subagent-control` stack.
