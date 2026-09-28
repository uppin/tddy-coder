# 2026-09-28 — A yielded conversation resumes with the caller's replacement call and result

PRD: the `resume-replacement` node of the `subagent-control` stack
([#557](https://github.com/uppin/tddy-coder/pull/557)).

When a yield condition hands a conversation back mid-call (see the turn-yield-conditions entry),
`subagent_resume` can carry a **`replacement`**: the caller substitutes its own `{tool, arguments,
result}` for the call that yielded. It is appended after any rewind and correction — the original
failed call stays in the history, nothing is rewritten — with minted ids, and the turn continues
over the appended history. The appended call **never dispatches**: its result is the caller's
text, recorded verbatim, so the model reads exactly the outcome the operator decided on.

A malformed replacement is refused before the turn runs — unknown tool, arguments the tool's own
schema refuses, or a result that is not bounded JSON — naming the offending field, so the refusal
never reshapes the history it is refused from.
