# Token-refusal message blames split-session key advertisement for a plain expiry

**Status:** open · **Found:** 2026-11-28

`packages/tddy-session-agents/src/exec_tool_caller.rs:31-41` folds every verification failure into
one message: "could not verify the session token (invalid or expired there); a split session's
tools run on the daemon holding the codebase…". The resolver `(user_resolver)(&req.session_token)`
returns `Option`, so the `SessionTokenError::Expired` from `session_token_v2.rs:405` is lost.

In session `01a20f10-…` the session was not split and the token was rejected on its own host
(LT-R1VXTH2V6H). The hint sent the agent, and the operator, looking at common-room key
advertisement. The real cause was a timestamp: `exp` 2026-11-27T21:18:35Z, first refusal 21:23:29Z.

**Fix sketch:** the resolver returns `Result<_, SessionTokenError>`. The message names the reason
(`expired at <exp>`, unknown key id, bad signature). For `Expired`, it also gives the remedy:
"the daemon re-mints on resume; ask the operator to resume this session".
