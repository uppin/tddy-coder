# Telegram session control (daemon library)

## Purpose

The **`tddy_daemon::telegram_session_control`** module defines the **inbound** side of Telegram-driven workflow control: command and callback parsing, plan text chunking for Telegram message size limits, presenter input byte encodings aligned with the web client, and a **`TelegramSessionControlHarness`** used in automated tests (and suitable for future wiring from a teloxide update loop).

This surface is **distinct** from **[Telegram session notifications](telegram-notifications.md)**, which cover **outbound** status and elicitation hints from the daemon's observer path.

## Daemon binary (inbound)

When **`telegram.enabled`** is true, a non-empty **`bot_token`** is set, and the daemon can resolve **`sessions_base`** for the current OS user (`USER` → `~/.tddy`), **`tddy-daemon`** starts a **teloxide long-polling** dispatcher (**`telegram_bot`**) alongside the web/RPC server. It handles the commands in the table above, inline **Enter** / **Delete** / **More** session-list callbacks (`enter:…`, `delete:…`, `more:…`), recipe / **branch-worktree intent** / project / **integration-base branch** / agent pick callbacks (`recipe:…`, `intent:…`, `tp:…`, `tb:…`, `ta:…`), document-review callbacks (`doc:…`), elicitation select callbacks (`eli:s:…`), elicitation **Other** (`eli:o:…`), and multi-select shortcut callbacks (**`eli:mn:`** **Choose none**, **`eli:mr:`** **Choose recommended** when the presenter supplies non-empty **`recommended_other`** on the MultiSelect wire). Commands and callbacks that drive the running **`tddy-coder`** child use **`PresenterIntent`** over **localhost** on the port registered for that session (see **`packages/tddy-daemon/src/presenter_intent_client.rs`**). The same **`TeloxideSender`** / **`Bot`** instance is shared with outbound notifications.

## Telegram user ↔ GitHub identity

The library module **`tddy_daemon::telegram_github_link`** binds a **Telegram user id** to a **GitHub login** (JSON store on disk, HMAC-signed OAuth **`state`**, stub OAuth exchange for tests). **`resolved_os_user_for_telegram_workflow`** resolves **`daemon.yaml`** **`users:`** the same way as web OAuth flows.

**`TelegramSessionControlHarness::with_telegram_github_link`** accepts a mapping file path. When that path is set, **`handle_start_workflow`** requires a stored GitHub login for the Telegram **`user_id`** before it creates a session directory. If the user is not linked, the handler fails with a message that instructs the operator to complete GitHub linking (including reference to **`/link-github`** in the error text).

Full-daemon wiring (OAuth callback **`state`** validation on the HTTP side, **`TelegramWorkflowSpawn`** OS user from the mapping, and Telegram commands that start the browser OAuth flow) integrates with **`DaemonConfig`** and **`AuthService`** at the binary layer. Technical reference: **[telegram-github-link.md](../../../packages/tddy-daemon/docs/telegram-github-link.md)**.

## Commands

| Command | Description |
|---------|-------------|
| **`/start-workflow <prompt>`** | Create a new session. Bot presents **Recipe: tdd-small** and **More recipes…**; **More recipes** sends a second message with **tdd**, **bugfix**, **free-prompting**, and **grill-me** (compact `mr:` callbacks). Choosing a recipe writes **`changeset.yaml`**, then the operator picks **branch/worktree intent** (new branch from integration base vs work on an existing branch — see below), then a **project**, then an **integration base** (see below), then an **agent** when the daemon **`allowed_agents`** list is non-empty. |
| **`/chain-workflow <prompt>`** | Create a **child** session for stacked work: the first outbound step is a **parent session picker** (other sessions under the same **`sessions_base`**, newest first, excluding the new child id). Inline buttons use **`tcp:<parent_idx>|s:<child_session_id>`** ( **`CB_TELEGRAM_CHAIN_PARENT`** prefix **`tcp:`** ) so **`callback_data`** stays within Telegram’s **64-byte** limit with a full child session id. A follow-on message presents the same **recipe** keyboard pattern as **`/start-workflow`**. Authorization and optional **Telegram ↔ GitHub** linking match **`handle_start_workflow`**. **`parse_telegram_chain_parent_callback`** decodes **`tcp:`** payloads for tests and future callback wiring. |
| **`/sessions`** | List sessions (10 at a time). Each session shows status, elapsed time, and workflow state. Paginated with a **"More"** inline keyboard button when more sessions exist. |
| **`/delete <session_id>`** | Delete a session. Bot sends SIGTERM/SIGKILL to the session process (if alive), removes the session directory, and confirms. |
| **`/submit-feature <session> <text>`** | Send feature description text to the running child presenter (**`PresenterIntent::SubmitFeatureText`**) when the workflow asks for it. |
| **`/answer-text <session> <text>`** | Free-text clarification answer (**`PresenterIntent::AnswerClarificationText`**). |
| **`/answer-multi <session> i,j,…`** | Multi-select clarification with 0-based indices (**`PresenterIntent::AnswerClarificationMultiSelect`**). |

### Concurrent elicitation (one Telegram chat, multiple sessions)

- **Per-chat queue:** The daemon maintains a **FIFO queue** of workflow session ids per Telegram chat. The **first** session in the queue holds the **active elicitation token** for that chat. Only that session receives full interactive treatment consistent with the “single visible question” policy for inbound and outbound Telegram.
- **Inbound gating:** **`eli:s:`**, **`eli:o:`**, **`eli:mn:`**, **`eli:mr:`**, and **`doc:`** (document-review) callbacks are accepted only when the callback’s session id matches the active token; other sessions receive a short alert. **`/answer-text`** and **`/answer-multi`** resolve the child gRPC target from the session key in the command, then apply the same active-token check before forwarding to **`PresenterIntent`**.
- **Plain-text follow-up (“Other”):** After the operator taps **Other** on a select clarification, the next non-command message in the chat is routed to the pending session when it matches the active token policy (see harness **`handle_elicitation_other_followup_plain_message`**).
- **Queue advancement:** After a successful step that completes the elicitation gate for the active session—including select confirmation, Other follow-up, multi-select shortcut answers (**`eli:mn:`** / **`eli:mr:`**), terminal document-review actions where applicable, and successful **`/answer-text`** / **`/answer-multi`**—the coordinator removes the completed session from the head of the queue and exposes the next session as active (when present).

### Session list behavior (`/sessions`)

- Sessions are listed **most recent first**, 10 per page.
- Each entry shows: short session label, status, workflow state, elapsed time.
- Each entry has inline keyboard buttons: **"Enter"** (connect to session workflow) and **"Delete"** (delete session with confirmation).
- When more than 10 sessions exist, a **"More"** button at the bottom loads the next page (offset-based pagination via callback data).
- The list reads from the same **`session_reader::list_sessions_in_dir`** and **`session_list_enrichment`** pipeline as **`ConnectionService::ListSessions`**.

### Session deletion (`/delete`)

- Delegates to the same **`session_deletion::delete_session_directory`** logic as **`ConnectionService::DeleteSession`**.
- If a live PID exists in **`.session.yaml`**, the daemon terminates the process before directory removal.
- Bot sends a confirmation message after successful deletion.

### Branch/worktree intent (`/start-workflow` after recipe)

- After a **recipe** is saved, the bot sends an inline keyboard with two options: **New branch + worktree** vs **Work on existing branch**. The choice is persisted immediately under **`changeset.yaml`** → **`workflow.branch_worktree_intent`** as **`new_branch_from_base`** or **`work_on_selected_branch`** (see **`BranchWorktreeIntent`** in **`tddy-core`** / workflow block in the changeset schema).
- **Callback_data** must stay within Telegram’s **64-byte** limit per button, so the wire format uses short tokens after the `intent:` prefix: **`intent:nb|s:<session_id>`** → `new_branch_from_base`, **`intent:ws|s:<session_id>`** → `work_on_selected_branch`. The session id is the daemon session directory name (typically a UUID).

### Integration base branch (`/start-workflow` after project)

- After a **project** is chosen (`tp:<proj_idx>|s:<session_id>`), the bot lists **Default (`<branch>`)** using **`effective_integration_base_ref_for_project`** (project registry **`main_branch_ref`**, else documented default **`origin/master`**), then up to **10** remote branches **`origin/...`** sorted by **most recent commit** (`git branch -r --sort=-committerdate`), exposed as **`list_recent_remote_branches`** in **tddy-core**.
- Callbacks: **`tb:0|p:<proj_idx>|s:<session_id>`** = use project default (no chain opt-in); **`tb:<n>|p:<proj_idx>|s:<session_id>`** with **`1 ≤ n ≤ 10`** = use the *n*th line from that sorted list. The choice is persisted to **`changeset.yaml`** as **`worktree_integration_base_ref`** when non-default; **`tddy-workflow-recipes`** / **`tddy-service`** worktree setup calls **`setup_worktree_for_session_with_optional_chain_base`** so the session worktree matches the selected base.

### Enter workflow

- **"Enter"** button on a session row connects to that session's workflow.
- Bot presents the current workflow state and available actions (plan review, elicitation responses, etc.) using inline keyboards.
- Presenter input from Telegram uses the same **`map_elicitation_callback_to_presenter_input`** encoding as the web client for legacy **`elicitation:`**-style payloads where applicable.

### Telegram-tracked session (inbound binding + replay)

- **`enter:<session_id>`** callbacks establish **per-chat tracking**: the Telegram **`chat_id`** binds to that workflow **`session_id`** inside a **`SharedTelegramTrackedSessionCoordinator`** shared with **`TelegramSessionWatcher`**, so outbound presenter keyboards and inbound elicitation gates agree on the operator’s chosen session.
- After a successful **Enter**, the control path **replays** pending presenter elicitation for that session when cached presenter state indicates an outstanding gate (test harnesses may attach a **`TelegramElicitationReplayBridge`** for the same replay contract without the full daemon graph).
- **Session delete** clears the tracked association when the deleted session id matches the chat’s tracked id. **WorkflowComplete** clears when the completed session matches the tracked pair.
- Integration coverage lives in **`packages/tddy-daemon/tests/telegram_tracked_session_acceptance.rs`** together with existing concurrent-elicitation and multi-select suites that bind tracking where full keyboards are asserted.

### Clarification (select, text, multi)

- **Single-select** (**`Select`**): the outbound notification lists each option on its own line in the message body; inline buttons use compact numeric labels. After the operator taps a button, the bot sends a **confirmation message** with the **full** chosen option text (label and description as defined by the presenter). The daemon resolves the choice via **`PresenterIntent::AnswerClarificationSelect`** on the child’s localhost gRPC port (see **`presenter_intent.proto`**).
- **Multi-select** (**`MultiSelect`**): the outbound path attaches an inline row with **Choose none** (**`eli:mn:<session_id>:<question_index>`**) and, when the presenter provides non-empty **`recommended_other`** on the clarification proto, **Choose recommended** (**`eli:mr:…`**). Each **`callback_data`** stays within Telegram’s **64-byte** limit. **Choose none** submits **`AnswerClarificationMultiSelect`** with empty indices and empty **Other**. **Choose recommended** submits empty indices and the cached **`recommended_other`** string as **Other** (no placeholder when the field is absent—the button is omitted).
- **Text** and index-based **multi-select** also use **`/answer-text`** and **`/answer-multi`** respectively (same **`PresenterIntent`** service).

### Document review on Telegram

When a session presents a document for **review / approve / reject**, the operator must see the **same document body on Telegram** as in the Virtual TUI or web, not only a one-line “review in the UI” hint:

- Send the **full document text** first (split across messages if needed; **`chunk_telegram_text`** applies to plain segments).
- Then send **Approve**, **Reject**, and **Refine** as an inline keyboard row (or the product’s equivalent labels aligned with the presenter), matching **[telegram-notifications.md](telegram-notifications.md)** — presenter stream: elicitation, document review.

Markdown adaptation for Telegram must respect **[message entities](https://core.telegram.org/api/entities)** rules (notably **UTF-16–based** offsets and lengths for styled spans). Prefer Bot API **`parse_mode`** with validated markup, or build **`entities`** explicitly after converting from session markdown.

## Library contents

| Area | Responsibility |
|------|----------------|
| **Commands** | **`parse_start_workflow_prompt`** extracts the prompt after a **`/start-workflow`** prefix. |
| **Callbacks** | **`parse_callback_payload`** recognizes recipe-style routing strings; recipe and intent selection merge into **`changeset.yaml`** inside the harness (**`parse_telegram_intent_callback`** for **`intent:`** payloads). |
| **Chunking** | **`chunk_telegram_text`** splits UTF-8 text with a newline and **`(continued)`** suffix on non-final segments when the byte budget allows continuation markers. |
| **Presenter bridge** | **`map_elicitation_callback_to_presenter_input`** produces **`PresenterInputPayload`** bytes matching the web encoding for single- and multi-select elicitation. Live workflows use **`PresenterIntent`** gRPC for answers and document actions. |
| **Persistence** | **`read_changeset_routing_snapshot`** reads **`recipe`**, **`demo_options`**, **`workflow.branch_worktree_intent`**, and **`run_optional_step_x`** from **`changeset.yaml`** for assertions. |
| **Harness** | **`TelegramSessionControlHarness`** creates a session directory under a configurable base path, sends an intro message with an inline recipe keyboard (test contract includes **`tdd-small`** labeling), applies recipe and intent callbacks to **`changeset.yaml`**, can show project/branch/agent pick keyboards when **`TelegramWorkflowSpawn`** is configured, sends plan review text in chunks via **`TelegramSender`**, and returns an explicit denial message for chat ids outside an allowlist. |
| **Test sender** | **`InMemoryTelegramSender`** (in **`telegram_notifier`**) implements **`TelegramSender`** including **`send_message_with_keyboard`** (row-major **`(label, callback_data)`** per button); **`collect_outbound_messages`** exposes structured **`CapturedTelegramMessage`** rows for tests. |

## Configuration and security (harness)

The harness takes an **allowed chat id list** and a **sessions base directory** supplied by the caller. Unauthorized chats receive a plain-text denial and do not create session directories. Full integration with **`DaemonConfig`**, bot tokens, and **`chat_ids`** in **`daemon.yaml`** belongs to the daemon binary when inbound control is enabled there.

## Operational scope

The daemon binary runs **long-polling** inbound handling (see above). Durable **chat ↔ session** registry across restarts and full **`ConnectionService` / `StartSession`** automation from Telegram remain follow-up work where needed.

## Tests

- **Unit tests** live in **`telegram_session_control.rs`** (`#[cfg(test)]`): parsers, chunking, presenter bytes.
- **Integration tests** live in **`packages/tddy-daemon/tests/telegram_session_control_integration.rs`**: start workflow, recipe **`changeset.yaml`**, branch/worktree intent keyboard and persistence, plan chunk markers, elicitation mapping, unauthorized denial.
- **Telegram ↔ GitHub linking** integration and unit tests live in **`packages/tddy-daemon/tests/telegram_github_link.rs`** and **`telegram_github_link.rs`** (`#[cfg(test)]`): OAuth state round-trip, mapping persistence, unlinked **`handle_start_workflow`** error path, stub exchange.
- **Concurrent elicitation** scenarios (single chat, multiple sessions, active token) live in **`packages/tddy-daemon/tests/telegram_concurrent_elicitation_integration.rs`**.
- **Multi-select shortcuts** (outbound keyboards, parser, metadata gating) live in **`packages/tddy-daemon/tests/telegram_multi_select_acceptance.rs`**.
- **Telegram-tracked session gate and replay** live in **`packages/tddy-daemon/tests/telegram_tracked_session_acceptance.rs`**.

## Related documentation

- **[telegram-notifications.md](telegram-notifications.md)** — outbound Telegram notifications and **`TelegramSessionWatcher`**.
- **[ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md)** — session lifecycle RPCs used by other clients (referenced for context; Telegram control does not replace these APIs in the current library scope).
