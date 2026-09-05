# 2026-03-08 — Agent Inbox

- **Inbox queue**: During Running mode, users type prompts and press Enter to queue them. Queued items display between the activity log and status bar.
- **Navigation**: Up/Down arrows (when input empty) move focus to inbox list; Up/Down navigate items; Esc returns to input.
- **Edit/Delete**: E on selected item enters edit mode (Enter saves, Esc discards); D removes the item.
- **Auto-resume**: On WorkflowComplete with non-empty inbox, the first item is dequeued and sent to the workflow thread. Agent receives an instruction prefix indicating items were queued.
- **Workflow loop**: The workflow thread loops after each cycle; waits for new prompt via channel; exits when channel closes.
- **Layout**: Inbox region has height 0 when empty or not in Running mode.
