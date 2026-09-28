# 2026-09-28 — A resume appends the caller's replacement call and result

**Type:** Feature

`#subagent-control` 5/5, [#557](https://github.com/uppin/tddy-coder/pull/557). Cross-package entry:
`docs/dev/changesets/2026-09-28-resume-replacement.md`.

`subagent/replacement.rs` holds the `Replacement {tool, arguments, result}` type and its
validation — the tool set **derived from the advertised definitions**, the arguments run through
the tool's own `validate_tool_arguments`, the result JSON-parsed and bounded, each rejection naming
its field, above the rewind so a malformed one never reshapes history.
`Transcript::append_replacement` records the call and its verbatim result with minted ids
(`call_replacement_{ordinal}`) — append-only, dispatching nothing — and the resume path appends
after any rewind and correction, before the turn loop runs over the appended history. An appended
replacement invalidates the repeat ledger like every outside-the-loop change.

Code issues at wrap: `oversized-file-conversation.md` 522 (+10), `oversized-file-subagent.md`
2,001 (+16) — inside the standing deferrals; split after the stack lands.
