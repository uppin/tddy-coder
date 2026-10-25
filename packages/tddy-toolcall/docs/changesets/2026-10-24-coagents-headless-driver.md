# 2026-10-24 — The untagged approve keeps its presenter path

**Type:** Test

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). A conversation-approval hook added and then removed in this PR left the listener as it was; `tests/untagged_approve_reaches_the_presenter.rs` pins that an `approve` with no conversation id is decided by the workflow presenter and its answer is returned.
