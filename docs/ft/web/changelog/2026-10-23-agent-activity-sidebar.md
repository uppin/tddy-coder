# 2026-10-23 — Agent activity opens as a right sidebar, by default for managed sessions

The Agent Activity transcript is now a right sidebar beside the session instead of a top-bar popover. It opens by default for sessions started in managed-codebase mode, whose agent works through daemon tools, and stays closed for the rest until toggled; a toggle lasts until another session is selected. `ListSessions` reports `managed_codebase` for every session started from now on.

See [Agent Activity Pane](../agent-activity-pane.md#right-sidebar-open-by-default-for-managed-sessions).
