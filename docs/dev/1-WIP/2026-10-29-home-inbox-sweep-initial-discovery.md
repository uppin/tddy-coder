# Initial discovery: sweep (`#home-inbox 11/11`)

## Conclusions

- **Backlog delta of `#home-inbox`** against master, taken on `origin/feature/home-inbox/terminal-styles` (`5dd74f09c`):
  - **Four TODO entries were added.** Three are resolved here. The fourth, cross-host wait, is `10/11`.
  - **Code-issue records modified:**
    - `exec_tool/project_home.rs`: 641 → 889.
    - `project_home_tools.rs`: 699 → 757.
    - tool-engine `lib.rs`: 885 → 888.
    - `terminal_capture.rs`: 668 → 712, below the 728 base after `9/11`.
    - `telegram_notifier.rs`: its test module only.
    - Three agent-launch complexity records, all improved.
    - `complexity-project-home-remote-home-caller-confirmed-at-owner`, deleted by `5/11`.
  - **No `TODO(<node>)` markers** remain in `src/`.
- **Tauri.** The lockfile has `tauri` 2.11.5. `packages/tddy-desktop/src-tauri/Cargo.toml` has `tauri = { version = "2" }`, and the notification plugin is pinned `~2.4`. The dev-shell `rustc` is 1.94.1, so the TODO's "Rust 1.90" requirement is already met. Only Tauri and the plugin move.
- **Spawner offer.** `is_conversation_spawner_tool` in `project_home_tools.rs` names the four start tools plus `AnswerSessionPermission` (`6/11`) and `AnswerSessionQuestion` (`8/11`). `ReadSessionActivity` is a project-home tool only.
- **Notify CLI.** `tddy-tools notify` from `1/11` uses `TDDY_HOOK_TOKEN` against `ReportOperatorNotification`. Standing-role launches skip the notify environment and guidance; the `role.name` / `session_role` gate lives in `claude_cli_spawn.rs` and `svc_resume_claude_cli_session.rs`.
- **The race.** `answer_permission` (`tddy-session-activity/src/answers.rs`) calls `clear_pending_question` unconditionally. `8/11` added the matching conditional clear for questions.

## Exploration 1 (2026-10-29)

These were read:
- the three TODO files;
- the code-issue records for `exec_tool/project_home.rs` (seams B and C) and `project_home_tools.rs` (seams A, B and C);
- `./dev rustc --version`;
- the lockfile and `Cargo.toml` entries for Tauri.
