# 2026-05-02 — Presenter clarification MultiSelect (empty answer guard)

- **`tddy-core`**: **`AnswerClarificationMultiSelect`** validation rejects empty selected indices when **Other** text is absent and **`allow_other`** on the active clarification question is **false** (aligns Telegram **Choose none** / index-based **`/answer-multi`** semantics with presenter gating).
