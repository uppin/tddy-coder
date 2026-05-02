# Changeset: Telegram MultiSelect shortcuts (State B)

**Date:** 2026-05-02  
**Type:** Feature (daemon + presenter + RPC wire + documentation)

## Product behavior

- **Outbound (`MultiSelect` clarification):** **`TelegramSessionWatcher`** attaches a compact shortcut row: **Choose none** (**`eli:mn:<session_id>:<question_index>`**) and **Choose recommended** (**`eli:mr:…`**) whenever **`recommended_other`** on **`ClarificationQuestionProto`** is non-empty (**Choose recommended** is omitted entirely when **`recommended_other`** is empty). **`callback_data`** complies with Telegram’s **64-byte** cap per keyboard button.
- **Metadata:** **`MultiSelectShortcutElicitationMeta`** (**session id**, **question index**, **`recommended_other`**) persists per Telegram chat plus session inside **`TelegramSessionWatcher`** so **Choose recommended** resolves without embedding long strings in **`callback_data`**.
- **Inbound (`telegram_bot`):** **`eli:mn:`** / **`eli:mr:`** share **`authorized_elicitation_surface_gate`** with **`eli:s:`**, **`eli:o:`**, **`doc:`**, and **`/answer-*`** parity — only the chat’s **primary** queued session succeeds; others receive deny feedback.
- **Presenter bridge:** Shortcut handlers invoke **`PresenterIntent::AnswerClarificationMultiSelect`**: **Choose none** submits empty indices with empty **Other**; **Choose recommended** submits empty indices with **`recommended_other`** as **Other**.
- **Presenter validation (`tddy-core`):** **`AnswerClarificationMultiSelect`** with empty indices requires non-empty **Other** text when **`allow_other`** on the clarification is **false** (empty answers are rejected deterministically).

## Automated tests

- **`packages/tddy-daemon/tests/telegram_multi_select_acceptance.rs`** — shortcut keyboards, parsers, **`recommended_other`** gating.
- **`packages/tddy-daemon/tests/telegram_concurrent_elicitation_integration.rs`** — deferred vs primary keyboards for MultiSelect shortcuts.

## Affected documentation

- **`docs/ft/daemon/telegram-session-control.md`** — callback inventory, clarification multi-select section, inbound gating, queue advancement.
- **`docs/ft/daemon/telegram-notifications.md`** — clarification **`MultiSelect`**, deferred primary keyboards, **`ActiveElicitationCoordinator`** wording.
- **`docs/ft/daemon/changelog.md`** — **`## 2026-05-02`** plus aligned **`eli:mn:`** / **`eli:mr:`** bullet under concurrent elicitation entry.
- **`docs/ft/coder/changelog.md`** — **`## 2026-05-02`** presenter MultiSelect validation.
- **`docs/dev/changesets.md`** — cross-package index (**top** wrapped list and **prepend-under-merge-hygiene** list).

## References

- [telegram-session-control.md](../../ft/daemon/telegram-session-control.md)
- [telegram-notifications.md](../../ft/daemon/telegram-notifications.md)
- `packages/tddy-daemon/src/telegram_multi_select_shortcuts.rs`
- `plans/evaluation-report.md` — risk snapshot (evaluate-changes tooling)
