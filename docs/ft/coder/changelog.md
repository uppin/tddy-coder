# Coder Changelog

Release note history for the Coder product area.

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-04-09 — Codex ACP backend (`codex-acp` agent)

- **tddy-core**: **`CodexAcpBackend`** speaks ACP to a **`codex-acp`** subprocess (mirrors **`ClaudeAcpBackend`**); session resume via **`load_session`**; OAuth retry path reuses **`codex login`** + **`codex_oauth_authorize.url`** when ACP reports auth-like errors and **`session_dir`** is set; **`agent-client-protocol`** **`=0.10.4`** with **`unstable`**. **`AnyBackend::CodexAcp`**, backend menu / CLI mapping for **`codex-acp`**; **`task.rs`** treats **`codex-acp`** like **`codex`** for **`codex_thread_id`** persistence.
- **tddy-coder**: **`--agent codex-acp`**, **`create_backend`** wiring, **`TDDY_CODEX_ACP_CLI`** override alongside existing Codex CLI env for OAuth helper.
- **tddy-acp-stub** / **tddy-integration-tests**: protocol bump; stub **`initialize`** advertises **`load_session`**; **`codex_acp_backend`** acceptance tests.
- **Docs**: [PRD-2026-04-09-codex-acp-backend.md](1-WIP/PRD-2026-04-09-codex-acp-backend.md); **[docs/dev/changesets.md](../../dev/changesets.md)**; package **`changesets.md`** for **tddy-core** and **tddy-coder**.

## 2026-04-06 — Codex OAuth relay foundations (daemon library; web UI)

- **tddy-daemon**: **`codex_oauth_relay`** validates authorize URLs and parses OAuth callbacks for future **`BROWSER`** capture and Codex listener relay (**`tddy-integration-tests`**: **`codex_oauth_web_relay_acceptance`**).
- **tddy-web**: **`CodexOAuthDialog`** for authorize URL display (iframe vs embedding-blocked link). Product doc: **[codex-oauth-web-relay.md](../web/codex-oauth-web-relay.md)**; daemon product doc: **[codex-oauth-relay.md](../daemon/codex-oauth-relay.md)**. Cross-package: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-06 — GitHub PR MCP tools (tddy-tools) and recipe prompts

- **tddy-tools**: MCP tools **`github_create_pull_request`** and **`github_update_pull_request`** (GitHub REST via **`curl`**); **`ServerInfo`** instructions name those tools when **`GITHUB_TOKEN`** or **`GH_TOKEN`** is set; mock-recorded request tests for JSON bodies and headers.
- **tddy-workflow-recipes**: **`github_rest_common`** holds shared **`Accept`**, **`X-GitHub-Api-Version`**, token resolution, and User-Agent strings for merge-pr curl and **tddy-tools**; **tdd-small** merged **`red`** prompt includes the GitHub PR tools section only with a non-empty token; merge-pr hooks continue to append GitHub PR tool awareness under the same condition.
- **Schema**: **`changeset-workflow`** accepts optional **`github_pr_tools_metadata`** alongside **`workflow`** fields.
- **Docs**: [github-pr-tools-mcp.md](github-pr-tools-mcp.md); [workflow-recipes.md](workflow-recipes.md); [workflow-json-schemas.md](workflow-json-schemas.md); **`packages/tddy-tools/docs/json-schema.md`**; package **`changesets.md`** for **tddy-tools** and **tddy-workflow-recipes**; **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-06 — Branch/worktree intent (changeset workflow)

- **tddy-core**: **`BranchWorktreeIntent`** (**`new_branch_from_base`** | **`work_on_selected_branch`**) on **`ChangesetWorkflow`**; **`branch_worktree_intent`** module (**`validate_workflow_branch_intent`**, **`resolve_branch_and_worktree_plan`**, **`merge_branch_worktree_intent_into_context`**); worktree setup paths **`setup_worktree_for_session_with_integration_base`** and **`setup_worktree_for_session_with_optional_chain_base`** apply intent when **`changeset.yaml`** **`workflow`** carries **`branch_worktree_intent`**; **`merge_persisted_workflow_into_context`** merges intent keys into engine **`Context`**.
- **tddy-tools**: **`changeset-workflow`** JSON Schema (**`branch_worktree_intent`**, **`selected_integration_base_ref`**, **`new_branch_name`**, **`selected_branch_to_work_on`**); **`persist-changeset-workflow`** round-trip validation.
- **tddy-service**: **`WorktreeElicitation`** optional fields align with **`changeset.yaml`** workflow (**`branch_worktree_intent`**, **`selected_integration_base_ref`**, **`new_branch_name`**, **`selected_branch_to_work_on`**).
- **Tests**: **`branch_worktree_intent_acceptance`**, **`branch_worktree_intent_red`** (**tddy-core**, **tddy-tools**).
- **Docs**: [workflow-json-schemas.md](workflow-json-schemas.md), [workflow-recipes.md](workflow-recipes.md), [planning-step.md](planning-step.md), [git-integration-base-ref.md](git-integration-base-ref.md); **[docs/dev/changesets.md](../../dev/changesets.md)**; package **`changesets.md`** for **tddy-core**, **tddy-tools**, **tddy-workflow-recipes**, **tddy-service**.

## 2026-04-05 — Default `free-prompting` session recipe and `/start-<recipe>` feature prompt

- **Sessions**: **New** sessions with no **`--recipe`** and no **`recipe`** in **`changeset.yaml`** use **`free-prompting`**. **`--recipe`** accepts **`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, **`grill-me`**, **`review`**, and **`merge-pr`** on the CLI.
- **TUI**: **FeatureInput** accepts **`/start-<cli>`** lines (supported recipe names); the slash menu lists **`/start-…`** rows before **`/recipe`** and project skills. After **`WorkflowComplete`** for a structured **`/start-*`** run (any recipe other than **`free-prompting`**), the active recipe returns to **`free-prompting`** and **`changeset.yaml`** stores **`free-prompting`** when resolution succeeds.
- **Docs**: [workflow-recipes.md](workflow-recipes.md) (**Feature prompt: `/start-<recipe>`**), [1-OVERVIEW.md](1-OVERVIEW.md), [feature-prompt-agent-skills.md](feature-prompt-agent-skills.md); package **`changesets.md`** for **tddy-core**, **tddy-coder**, **tddy-workflow-recipes**, **tddy-tui**; cross-package **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Chain PR optional integration base (worktrees)

- **tddy-core**: **`validate_chain_pr_integration_base_ref`**, **`fetch_chain_pr_integration_base`**, **`setup_worktree_for_session_with_optional_chain_base`**, **`resolve_persisted_worktree_integration_base_for_session`**; **`Changeset`** fields **`effective_worktree_integration_base_ref`**, **`worktree_integration_base_ref`** on **`changeset.yaml`**.
- **tddy-integration-tests**: **`chain_pr_base_acceptance`** (default base, selected **`origin/...`** base, persistence, validation, resume resolution).
- **Docs**: [git-integration-base-ref.md](git-integration-base-ref.md); **`packages/tddy-core/docs/architecture.md`**, **`packages/tddy-core/docs/changesets.md`**; cross-package **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Review workflow recipe (`review`)

- **Workflow recipes**: **`ReviewRecipe`** — graph **`inspect` → `branch-review` → `end`**; **`ReviewWorkflowHooks`** merge-base and bounded **`git diff`** context; **`SessionArtifactManifest`** maps **`review` → `review.md`**; **`approval_policy`** includes **`review`** in supported CLI names and session-document skip rules.
- **JSON Schema**: **`goals.json`** registers **`branch-review`** with **`generated/tdd/branch-review.schema.json`** and **`proto/branch_review.proto`**.
- **tddy-tools**: **`submit --goal branch-review`** validates payloads, writes **`review.md`** under **`TDDY_SESSION_DIR`** when set, then relays when **`TDDY_SOCKET`** is present.
- **tddy-coder**: **`--recipe review`** resolves **`ReviewRecipe`**; **`--goal`** accepts **`inspect`** and **`branch-review`** for that recipe.
- **tddy-daemon / Telegram**: Extended recipe keyboard list includes **`review`** (normalized CLI name) where **`RECIPE_MORE_PAGE`** applies.
- **Docs**: [workflow-recipes.md](workflow-recipes.md), [workflow-json-schemas.md](workflow-json-schemas.md); **`packages/tddy-workflow-recipes/docs/changesets.md`**, **`packages/tddy-tools/docs/changesets.md`**, **`packages/tddy-coder/docs/changesets.md`**, **`packages/tddy-daemon/docs/changesets.md`**; cross-package **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Documentation wrap (grill-me PRD retired)

- **Docs**: WIP PRD for **grill-me** removed from **`docs/ft/coder/1-WIP/`**; product behavior remains in [workflow-recipes.md](workflow-recipes.md). Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-04 — TDD changeset workflow and demo routing persistence

- **Changeset**: **`ChangesetWorkflow`** on **`changeset.yaml`** under **`workflow`** (**`run_optional_step_x`**, **`demo_options`**, optional **`tool_schema_id`**); **`write_changeset_atomic`**; **`merge_persisted_workflow_into_context`** applies persisted values to engine **`Context`**.
- **tddy-tools**: **`persist-changeset-workflow`** (**`--session-dir`**, **`--data`**) validates **`changeset-workflow`** JSON and writes the **`workflow`** block; registry entry in **`goals.json`** with **`generated/tdd/changeset-workflow.schema.json`** and **`proto/changeset_workflow.proto`**.
- **Interview**: Prompts require **`tddy-tools ask`** for optional **demo** after **green**, demo options, and persistence of **`run_optional_step_x`** / **`demo_options`** into **`changeset.yaml`**.
- **Tests**: **`changeset_demo_workflow_acceptance`**, **`changeset_workflow_cli`**, **`tdd_demo_workflow_config`**, **`workflow_graph`** **`full_graph_green_routes_per_changeset_demo_intent`**; **`cli_integration`** lists **`changeset-workflow`** in **`REGISTERED_GOALS`**.
- **Docs**: [workflow-recipes.md](workflow-recipes.md), [planning-step.md](planning-step.md), [workflow-json-schemas.md](workflow-json-schemas.md); **`packages/tddy-tools/docs/json-schema.md`**; **`packages/tddy-workflow-recipes/docs/workflow-schemas.md`**; package **`changesets.md`** for **tddy-core**, **tddy-tools**, **tddy-workflow-recipes**.

## 2026-04-04 — TDD interview step before plan

- **Workflow recipes**: **`TddRecipe`** start goal **`interview`**; graphs **`interview` → `plan` → …**; relay **`.workflow/tdd_interview_handoff.txt`** into **`plan`** via **`answers`**; **`goal_requires_tddy_tools_submit`** **`false`** for **`interview`**.
- **Core**: **`WorkflowRecipe::plan_refinement_goal()`** (default **`start_goal()`**); **`TddRecipe`** refinement target **`plan`**; **`GrillMeRecipe`** refinement target **`create-plan`**; **`run_plan_refinement`** in **`tddy-coder`** uses **`plan_refinement_goal()`** for session tag lookup and **`run_goal`**.
- **CLI / hooks**: Session id preservation when a workflow **`session_id`** is already bound in **`before_interview`**, **`before_acceptance_tests`**, **`before_red`**; failed-resume helpers for TDD **`start_goal`** / **`plan`** alignment.
- **Tests**: Integration and recipe acceptance tests for graph topology, handoff, **`backend_invoke_no_tddy_tools_submit`** parity for **`interview`**; e2e presenter tests account for **`Interviewing`** / **`Interviewed`** transitions.
- **Docs**: [workflow-recipes.md](workflow-recipes.md), [planning-step.md](planning-step.md), [implementation-step.md](implementation-step.md); package **`changesets.md`** entries for **tddy-core**, **tddy-coder**, **tddy-workflow-recipes**.

## 2026-04-04 — Activity log: user prompt lines and TUI presentation

- **Core**: **`ActivityKind::UserPrompt`** marks submitted feature text and queued inbox lines in **`activity_log`** / **`ActivityLogged`**. **`format_user_prompt_line`** returns plain text (no `User: ` prefix); **`format_queued_prompt_line`** keeps the **`Queued: `** prefix. **`tddy-service`** maps the kind to the **`UserPrompt`** string for RPC consumers.
- **TUI**: User prompt entries render as a three-row inset block (margins on all sides): first row empty, text on rows two and three with hard wrap and ellipsis when needed; panel **`Rgb(85, 85, 85)`**, text **`Rgb(255, 255, 255)`** bold.
- **Tests**: **`presenter_integration`** expects exact submitted text; **`tddy-tui`** unit tests for user-prompt row layout helpers.
- **Docs**: [Activity log streaming](activity-log-streaming.md); **`packages/tddy-core/docs/architecture.md`**; **`packages/tddy-tui/docs/architecture.md`**.

## 2026-03-29 — Activity log: user prompts and incremental agent streaming

- **Core**: **`presenter::activity_prompt_log`** (**`User: `** / **`Queued: `** prefixes) wires **`SubmitFeatureInput`** and **`QueuePrompt`** into **`activity_log`** and **`ActivityLogged`**. **`presenter::agent_activity`** holds incremental tail helpers and channel policy constants. **`Presenter::poll_workflow`** on **`WorkflowEvent::AgentOutput`** maintains a growing partial **`AgentOutput`** row in **`activity_log`**, finalizes completed lines at newline boundaries, and broadcasts each chunk via **`PresenterEvent::AgentOutput`** without duplicating routine streaming text on **`ActivityLogged`**.
- **Tests**: Presenter unit tests in **`tddy-core`**; **`presenter_integration`** acceptance tests for user and queued prompt lines.
- **Docs**: [Activity log streaming](activity-log-streaming.md), [overview](1-OVERVIEW.md), **`packages/tddy-core/docs/architecture.md`**.

## 2026-04-04 — Bugfix recipe: `analyze` start goal and structured submit

- **Recipes**: **`BugfixRecipe`** graph is **`analyze` → `reproduce` → `end`**; **start goal** **`analyze`**; **`analyze`** uses **`tddy-tools submit`** with JSON Schema **`analyze`** (`branch_suggestion`, **`worktree_suggestion`**, optional **`name`**, optional **`summary`**); **`summary`** is available to **`reproduce`** via **`changeset.artifacts["analyze_summary"]`**; **`reproduce`** has **`goal_requires_tddy_tools_submit`** **`false`**; **`uses_primary_session_document`** is **`false`** (manifest still lists **`fix-plan.md`**).
- **Registry**: **`goals.json`** includes **`analyze`**; **`tddy-tools`** embeds the schema.
- **Docs**: [workflow-recipes.md](workflow-recipes.md) (BugfixRecipe and developer reference); [workflow-json-schemas.md](workflow-json-schemas.md) (registry summary lists **`analyze`**).

## 2026-04-04 — TDD-small workflow recipe and `post-green-review` schema

- **Recipes**: **`TddSmallRecipe`** (**`tdd-small`**) — graph **`plan` → `red` → `green` → `post-green-review` → `refactor` → `update-docs` → `end`**; merged red prompt path; single **`post-green-review`** structured submit for evaluate/validate-style fields; **`TddSmallWorkflowHooks`** with shared helpers alongside classic TDD hooks.
- **Registry**: **`goals.json`** includes **`post-green-review`** with **`generated/tdd/post-green-review.schema.json`** and **`proto/post_green_review.proto`**; **`tddy-tools`** **`get-schema post-green-review`**, **`list-schemas`**, and validated **`submit`** use the same registry.
- **CLI**: **`--recipe tdd-small`**; **`--goal`** accepts **`post-green-review`** where the active recipe defines it.
- **Docs**: [workflow-recipes.md](workflow-recipes.md), [workflow-json-schemas.md](workflow-json-schemas.md); **`packages/tddy-workflow-recipes/docs/changesets.md`**.

## 2026-04-04 — Git integration base ref for worktrees

- **tddy-core**: **`validate_integration_base_ref`**, **`fetch_integration_base`**, **`setup_worktree_for_session_with_integration_base`**, **`resolve_default_integration_base_ref`**, **`DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`** (`origin/master` for legacy rows); **`setup_worktree_for_session`** resolves default remote refs (`origin/master` → `origin/main` → **`origin/HEAD`**) after **`git fetch origin`**, then delegates to **`setup_worktree_for_session_with_integration_base`**; **`fetch_origin_master`** delegates to **`fetch_integration_base`** with the documented default.
- **tddy-daemon**: **`ProjectData.main_branch_ref`** (optional); **`effective_integration_base_ref_for_project`**; **`add_project`** validates **`main_branch_ref`** before persistence.
- **Tests**: **`tddy-core`** worktree integration tests; **`tddy-daemon`** project storage acceptance tests; **`tddy-integration-tests`** **`worktree_uses_configured_project_base_ref`**.
- **Docs**: [git-integration-base-ref.md](git-integration-base-ref.md); **tddy-core** / **tddy-daemon** package **`changesets.md`**; [project concept](../daemon/project-concept.md) data model.
- **Documentation wrap**: WIP changesets and validate-report folders under **`docs/dev/1-WIP/`** removed after transfer; **daemon** PRD removed with role summary merged into [project concept](../daemon/project-concept.md) and [daemon changelog](../daemon/changelog.md).

## 2026-04-03 — TUI Stop pane (interrupt beside Enter)

- **tddy-tui**: **`right_chrome_reserve_cols`** reserves **four** or **eight** columns for Enter-only vs Enter+Stop; **`stop_button_rect`** / **`paint_stop_affordance`** (red **U+25A0**); left-click → **`UserIntent::Interrupt`** → **`ctrl_c_interrupt_session`** (not sent to presenter). **`VirtualTui`** and local **`event_loop`** handle **`Interrupt`** like **Ctrl+C**.
- **tddy-tui**: **`ctrl_c_interrupt_session`** only kills the tracked backend child (**`kill_child_process`**); it no longer sets the workflow **`shutdown`** flag, so Stop / TUI **Ctrl+C** / byte **0x03** do not exit the full TUI run (SIGINT via **`ctrlc`** still tears down the runner).
- **tddy-core**: **`UserIntent::Interrupt`** (presenter no-op for exhaustiveness).
- **Docs**: [tui-status-bar.md](tui-status-bar.md) (**Mouse mode: Stop control**).

## 2026-03-29 — Free prompting: invoke loop, optional submit, and activity pane

- **Core**: **`WorkflowRecipe::goal_requires_tddy_tools_submit`** (default **`true`**); **`BackendInvokeTask`** completes a turn from agent **`InvokeResponse::output`** when the recipe opts out for that goal; **`FlowRunner`** maps **`Continue`** with no next graph task to **`WaitingForInput`** so a single-node graph can take another user line without **`EndTask`**.
- **Recipes**: **`FreePromptingRecipe`** graph is one **`BackendInvokeTask`** for **`prompting`**; **`goal_requires_tddy_tools_submit`** is **`false`** for **`prompting`**; **`FreePromptingWorkflowHooks::agent_output_sink`** emits **`WorkflowEvent::AgentOutput`** for streaming assistant text to the TUI activity pane.
- **Stub**: **`StubBackend`** **`response_for_goal`** includes a **`prompting`** arm for deterministic tests.
- **Docs**: [workflow-recipes.md](workflow-recipes.md).

## 2026-03-29 — Workflow recipes: free-prompting and recipe-driven session document policy

- **Recipes**: **`FreePromptingRecipe`** (**`free-prompting`**): **Prompting** loop with start goal **`prompting`**; **`uses_primary_session_document`** **`false`** (no PRD-style primary-document approval gate on that path).
- **Registry**: **`tddy-workflow-recipes::approval_policy`** (**`supported_workflow_recipe_cli_names`**, **`recipe_should_skip_session_document_approval`**); **`recipe_resolve`** resolves **`free-prompting`**; **`unknown_workflow_recipe_error`** enumerates supported CLI names.
- **Bootstrap**: Presenter **`workflow_runner`** **`run_start_goal_without_output_dir`** writes **`recipe`** into **`changeset.yaml`** when creating the session directory (same field as daemon **`StartSession`** and CLI session bootstrap).
- **CLI / TUI / web**: **`--recipe free-prompting`**; **`workflow_recipe_selection_question`** includes **Free prompting**; **`recipe_cli_name_from_selection_label`** maps the label to **`free-prompting`**.
- **Tests**: **`workflow_recipe_acceptance`**, **`recipe_policy_red`**, **`presenter_integration`** acceptance cases for TDD document approval, bugfix **`reproduce`**, and free-prompting without **DocumentReview**; **`grpc_terminal_rpc`** UTF-8-safe assertion previews.
- **Docs**: [workflow-recipes.md](workflow-recipes.md).

## 2026-04-03 — TUI bottom chrome: user prompt strip, footer row, Enter affordance

- **Layout**: **`layout_chunks_with_inbox`** allocates a **footer** row below the prompt block, a **separator row** below the status bar, and **`LayoutAreas`** includes **`footer_bar`** and **`enter_pane`**.
- **Activity (Running)**: The last line of the activity log shows **`> {running_input}`** with **white** foreground on **dark grey** background when follow-up text is non-empty.
- **Mouse**: **`enter_button_rect`** is **three** columns wide to the right of the prompt text (with margin); height spans from the first row **below the status bar** through prompt **text** lines and the **footer** (the bottom prompt **rule** row is excluded when present). **`paint_enter_affordance`** draws a light box frame with **U+23CE** on the first prompt text row. **`TDDY_E2E_NO_ENTER_AFFORDANCE`** skips overlay paint for stable byte-level tests. **`ViewState::last_select_click_option`** supports double-click to confirm in **Select** mode.
- **Tests**: **`packages/tddy-tui`** layout, rect geometry, strip styling, and env-gate tests; **`tddy-e2e`** **`grpc_terminal_rpc`** where terminal streaming uses the env gate.
- **Docs**: [tui-status-bar.md](tui-status-bar.md); [web-terminal.md](../web/web-terminal.md#connected-terminal-ux); **`packages/tddy-tui/docs/changesets.md`**.

## 2026-03-29 — Feature prompt: project agent skills and `/recipe`

- **Core**: **`tddy_core::agent_skills`** scans **`.agents/skills/<folder>/SKILL.md`**, parses YAML frontmatter (**`name`**, **`description`**), rejects folder/name mismatches with **`InvalidSkillEntry`**, exposes **`slash_menu_items`** (**`BuiltinRecipe`** plus valid skills), **`compose_prompt_with_selected_skill`** (PRD-shaped block with path and body), and **`agents_skills_scan_cache_token`** for scan invalidation hints.
- **Presenter**: **`apply_feature_slash_builtin_recipe`**, **`recipe_slash_selection_active`**, **`with_recipe_resolver`**; **`workflow_recipe_selection_question`** and **`recipe_cli_name_from_selection_label`** in **`backend`**. **`tddy-coder`** **`run.rs`** attaches the resolver for daemon and full TUI presenters.
- **Tests**: **`prompt_slash_skills_acceptance`**, **`prompt_slash_skills_lower`**, unit tests in **`agent_skills.rs`**.
- **Docs**: [Feature prompt: agent skills](feature-prompt-agent-skills.md), [overview](1-OVERVIEW.md), **`packages/tddy-core/docs/architecture.md`**.

## 2026-03-29 — TUI status bar: worktree segment, idle heartbeat, plan tail, prompt caret

- **Core**: `PresenterState::active_worktree_display` carries a short worktree label from `WorkflowEvent::WorktreeSwitched` via `presenter::worktree_display::format_worktree_for_status_bar`.
- **TUI**: The status line prefixes activity, session segment, and optional worktree label before `Goal:`; idle waits use a multi-phase `·`/`•`/`●` heartbeat while agent-active mode keeps the fast spinner; goal elapsed stays frozen in clarification waits as documented in [tui-status-bar.md](tui-status-bar.md).
- **Markdown plan viewer**: Approve and Reject appear as trailing wrapped lines after the user scrolls to the end of the plan body; scroll bounds use `Paragraph::line_count` with the same wrap as draw (ratatui `unstable-rendered-line-info`).
- **Local TUI**: The hardware cursor sits at the UTF-8-safe insert index for prompt editing; crossterm `Show` runs when a caret position applies.
- **Virtual TUI / streaming**: Cursor-position-only CSI updates respect a minimum send interval; full frames still diff on paint changes.
- **Docs**: [tui-status-bar.md](tui-status-bar.md); `packages/tddy-tui/docs/architecture.md`; `packages/tddy-core/docs/architecture.md`.

## 2026-03-29 — OpenAI Codex CLI backend

- **Core**: `CodexBackend` implements `CodingBackend` with `codex exec` and `codex exec resume <id>`, `--json` JSONL stdout, `-C` working directory, `-m` model; sandbox and approval flags derived from recipe `GoalHints` (read-only plan goals use read-only sandbox; editing goals use workspace-write; `--ask-for-approval never` for non-interactive runs).
- **Prompt**: System instructions merge into the user prompt with the same precedence as Cursor (`system_prompt_path` over inline `system_prompt`).
- **CLI**: `--agent codex`; `--codex-cli-path` and `TDDY_CODEX_CLI`; YAML `codex_cli_path` in coder config; `tddy-tools` availability required for codex like claude and cursor.
- **Selection**: Interactive menu order Claude → Claude ACP → Cursor → Codex → Stub; default model label `gpt-5` for agent key `codex`.
- **Tests**: Unit tests in `tddy-core`; stub-based integration tests in `tddy-integration-tests` (`codex_backend`); CLI acceptance for `--agent codex`.
- **Docs**: [Coder overview](1-OVERVIEW.md), [planning step](planning-step.md), [implementation step](implementation-step.md); cross-package index `docs/dev/changesets.md`; `packages/tddy-core/docs/architecture.md` and package `changesets.md` files (`tddy-core`, `tddy-coder`).

## 2026-03-29 — Web daemon: stub OAuth when stub codes are set

- **`tddy-coder`**: **`build_auth_service_entry`** treats non-empty **`--github-stub-codes`** (after trim) as stub auth mode alongside **`--github-stub`**, wiring **`StubGitHubProvider`** and optional code→user mappings for automated browser sign-in (e.g. Cypress **`app-connect`** flows).
- **Operational note**: Production-style launches must omit stray **`--github-stub-codes`** values unless stub authentication is deliberate.
- **Feature / cross-package**: [web-terminal.md](../web/web-terminal.md) (connection flows); [web changelog](../web/changelog.md) **2026-03-29**.

## 2026-03-28 — Workflow goal conditions and session context

- **Engine**: Workflow transitions evaluate declarative **`goal_conditions`** against **`Context`**. **`Context::merge_json_object_sync`** applies session JSON so predicates see the same keys as the persisted session file.
- **CLI**: **`tddy-tools set-session-context`** merges JSON into **`.workflow/<session-id>.session.json`** using **`TDDY_SESSION_DIR`** and **`TDDY_WORKFLOW_SESSION_ID`**. The subcommand is **not** registered in **`goals.json`** (session utility, not a schema-backed planning goal).
- **TDD graph**: Boolean session key **`run_optional_step_x`** selects the demo branch after green; recipe hooks use **`tddy-tools ask`** and **`set-session-context`** to record the choice.
- **Docs**: [workflow-recipes.md](workflow-recipes.md) (session context section), [workflow-json-schemas.md](workflow-json-schemas.md) (`set-session-context`), [implementation-step.md](implementation-step.md) (demo workflow and state machine).

## 2026-03-28 — Session directory layout (unified `sessions/<id>/`)

- **Contract**: Plan and workflow state use `{sessions_base}/sessions/{session_id}/`; process-bound session id takes precedence over backend-reported ids where they differ (`tddy_core::session_lifecycle`).
- **Presenter**: The workflow runner resolves `session_dir` from engine context or materializes from `session_base` + `session_id`; missing both yields a clear workflow error (no anonymous fallback directory).
- **Docs**: [Session directory layout](session-layout.md) (including [migration from non-unified trees](session-layout.md#migration-from-non-unified-trees)).

## 2026-03-28 — Bugfix workflow recipe (selectable `tdd` / `bugfix`)

- **Recipes**: **`tddy-workflow-recipes::recipe_resolve`** provides **`workflow_recipe_and_manifest_from_cli_name`** and **`resolve_workflow_recipe_from_cli_name`**; **`tddy-coder`** uses **`--recipe`** and optional config **`recipe:`**; **`changeset.yaml`** optional **`recipe:`** for resume; default **`tdd`** when unset.
- **BugfixRecipe**: Start goal **`reproduce`**; primary session document **`fix-plan.md`**; approval gate before **green**; **`uses_primary_session_document`** **`true`**.
- **Daemon / web**: **`StartSession` / `StartSessionRequest`** **`recipe`** field; **`tddy-daemon`** passes **`--recipe`** to spawned **`tddy-coder`**; **`ConnectionScreen`** workflow recipe dropdown on **Start New Session**.
- **Docs**: [workflow-recipes.md](workflow-recipes.md) ([Developer reference (TDD vs Bugfix)](workflow-recipes.md#developer-reference-tdd-vs-bugfix)).

## 2026-03-28 — TUI status bar: idle wait vs agent activity

- **Activity**: In `Running`, the status line uses the fast spinner and live goal elapsed. In clarification waits (`Select`, `MultiSelect`, `TextInput`), the displayed goal elapsed is frozen and the leading indicator is a one-second ·/• pulse; `VirtualTui` periodic renders align (~200 ms vs ~1 s) so streamed frames match local behavior without unnecessary traffic during waits.
- **Code**: `status_bar_activity` module; `ViewState` anchors for frozen elapsed and idle phase; shared `draw()` for local and remote.
- **Docs**: [tui-status-bar.md](tui-status-bar.md), `packages/tddy-tui/docs/architecture.md`.

## 2026-03-28 — Recipe-owned session artifacts (core decoupling)

- **Behavior**: Primary planning document paths and **`session_dir/artifacts/`** layout are driven by **`WorkflowRecipe`** + **`SessionArtifactManifest`** and **`tddy-workflow`** path helpers, not hard-coded **`PRD.md`** defaults in **`tddy-core`**.
- **API**: **`WorkflowRecipe::uses_primary_session_document`** / **`read_primary_session_document_utf8`** for approval, CLI, and daemon; TDD recipe behavior unchanged (**`prd` → `PRD.md`** in manifest).
- **Docs**: [workflow-recipes.md](workflow-recipes.md) (session artifacts section), package architecture notes under **`tddy-core`**, **`tddy-workflow`**, **`tddy-workflow-recipes`**.

## 2026-03-28 — Workflow JSON Schemas (tddy-tools + tddy-workflow-recipes)

- **Registry**: `packages/tddy-workflow-recipes/goals.json` lists each CLI goal with schema filename and proto basename; build output includes `generated/schema-manifest.json` and generated proto basename tables.
- **tddy-tools**: Embeds schemas from `tddy-workflow-recipes/generated/`; subcommands `get-schema`, `list-schemas`, and validated `submit`; 16 MiB cap on stdin/`--data` for submit and ask.
- **Documentation**: [workflow-json-schemas.md](workflow-json-schemas.md), package notes under `packages/tddy-tools/docs/json-schema.md` and `packages/tddy-workflow-recipes/docs/workflow-schemas.md`.

## 2026-03-28 — TUI status bar: spinner and session segment

- **Status line**: The activity spinner lives in the status bar before `Goal:` (not a separate top-right cell). A short segment derived from the workflow engine session id appears between the spinner and `Goal:` when the id matches UUID first-field rules; otherwise a fixed em-dash placeholder keeps the column stable.
- **Presenter**: `PresenterState::workflow_session_id` reflects `SessionStarted` and `start_workflow`; it clears on workflow completion and before inbox-driven restarts.
- **Remote parity**: Virtual TUI and gRPC terminal streams use the same `draw()` formatting as the local TUI.
- **Docs**: [tui-status-bar.md](tui-status-bar.md), `packages/tddy-tui/docs/architecture.md`.

## 2026-03-22 — Workflow recipes (pluggable workflows)

- **Architecture**: `GoalId` and string-backed workflow state; **`WorkflowRecipe`** in **`tddy-core`**; concrete recipes in **`tddy-workflow-recipes`** (`TddRecipe`, **`BugfixRecipe`** stub). Graph, hooks, permissions, and backend hints are recipe-defined.
- **CLI**: `--goal` validation uses the active recipe’s goal list.
- **Docs**: [workflow-recipes.md](workflow-recipes.md).

## 2026-03-22 — web-dev: daemon-only

- **Local web stack**: `./web-dev` runs **`tddy-daemon`** with **`dev.daemon.yaml`** at the repo root when **`DAEMON_CONFIG`** is unset; **`DAEMON_CONFIG`** selects another YAML path. The script resolves the debug or release daemon binary, writes a temp YAML with **`CURRENT_USER`** substituted, passes through CLI arguments, derives **`DAEMON_PORT`** from that YAML for the Vite **`/rpc`** proxy, and starts **`packages/tddy-web`** via Vite under **`./dev`**.
- **Daemon config vs `.env` `CONFIG`**: The daemon YAML path comes from **`DAEMON_CONFIG`** / **`dev.daemon.yaml`**, not from the generic **`CONFIG`** variable that `.env` may set for other tools.
- **Docs**: [Local web dev](../web/local-web-dev.md) describes the flow, env vars, and contract tests.

## 2026-03-22 — Red phase: production-only logging markers

- **Structured output**: Red JSON may include `source_file` per logging marker (where the marker was placed). `tddy-tools submit` validates against the updated `red` schema.
- **Enforcement**: `tddy-core` rejects red output when `source_file` points at test-only paths (Rust integration-test trees and `*_test.rs` file names). Agents must place markers on production skeleton entry points, not in test-only files.
- **Packages**: tddy-core (`source_path`, parser validation, red workflow prompt), tddy-tools (embedded schema).

## 2026-03-21 — Interactive backend selection

- **CLI**: `--agent` is optional. When omitted, choose backend via TUI dropdown or plain stdin menu before the workflow; when set, behavior matches the previous default path.
- **Defaults**: Cursor uses `composer-2`; `--model` overrides per-backend defaults and is passed to `cursor agent` as `--model`.
- **tddy-demo**: Still defaults to stub when `--agent` is omitted (no interactive menu).
- **Product reference**: [Coder overview — Backend selection](1-OVERVIEW.md#backend-selection-at-session-start).

## 2026-03-21 — Feature docs: transport PRDs wrapped

- **Consolidated** WIP PRDs for LiveKit participant, dual-transport codegen, Connect-RPC transport, and TokenService into [gRPC remote control](grpc-remote-control.md) (transport stack section) and this changelog. Source PRDs moved to `docs/ft/coder/1-WIP/archived/`.

## 2026-03-21 — Daemon session: `--project-id`

- **CLI**: `--project-id` on `tddy-coder` / `tddy-demo` when spawned by the daemon; persisted in `SessionMetadata` (`.session.yaml`).
- **Integration**: See [daemon project concept](../daemon/project-concept.md).

## 2026-03-19 — ACP Backend Implementation

- **ClaudeAcpBackend**: New `CodingBackend` that speaks ACP (Agent Client Protocol) to `@zed-industries/claude-agent-acp` subprocess via agent-client-protocol Rust SDK. Dedicated thread with LocalSet for !Send SDK. Session mapping (Fresh/Resume), progress events (TaskProgress, ToolUse, TaskStarted).
- **tddy-acp-stub**: New crate implementing `acp::Agent` for testing. Scenario-based responses (chunks, tool_calls, permission_requests).
- **CLI**: `--agent claude-acp` selects ACP backend. `verify_tddy_tools_available` skips check for claude-acp.
- **Packages**: tddy-core (backend/acp.rs), tddy-acp-stub (new), tddy-coder (run.rs).

## 2026-03-19 — Configurable Log Routing via YAML Config

- **Log config section**: YAML `log:` section with named loggers (output target + format) and policies that reference loggers by name. Selectors: target (exact/glob), module_path, heuristic. First-match-wins ordering.
- **CLI**: `--log-level <level>` overrides default policy level. Removed `--debug`, `--debug-output`, `--webrtc-debug-output`.
- **Log rotation**: On startup, existing log files renamed with timestamp suffix; rotated files beyond `max_rotated` pruned.
- **Packages**: tddy-core (log_backend.rs), tddy-coder (config.rs, run.rs).

## 2026-03-18 — Debug, Demo Worktree, Workflow Logging

- **WebRTC debug output** (superseded by log config): Previously `--webrtc-debug-output <path>` routed libwebrtc logs to a separate file; now use `log:` section with `selector: { target: "libwebrtc" }`.
- **Demo worktree skip**: When backend is stub (tddy-demo), acceptance-tests uses output_dir directly; no git fetch or worktree creation.
- **Workflow failure logging**: Workflow failures logged at error level for visibility in debug output.
- **VirtualTui debug logs**: Input, keys, mouse, resize, render, frame sent at debug level for remote TUI troubleshooting.
- **web-dev**: Passes CLI args to daemon binary.
- **Packages**: tddy-core (log_backend, tdd_hooks, presenter), tddy-coder (Args, init_tddy_logger), tddy-tui (virtual_tui), tddy-web (mobile keyboard overlay).

## 2026-03-18 — Terminal Resize Support

- **Local event loop**: Handles `Event::Resize` with `terminal.clear()` for a clean redraw with no visual artifacts.
- **Virtual TUI**: Accepts `\x1b]resize;cols;rows\x07`; after `terminal.resize()` calls `terminal.clear()` and resets the frame buffer so the next render sends a full frame to the remote client.
- **Scroll offset**: Clamped after resize so content does not jump past the end.
- **Packages**: tddy-tui (event_loop.rs, virtual_tui.rs).

## 2026-03-14 — Per-Connection Virtual TUI

- **Presenter view decoupling**: `connect_view()` → ViewConnection (state snapshot + event_rx + intent_tx). NoopView for headless/daemon.
- **VirtualTui**: Headless ratatui per connection; CapturingWriter headless(); event subscription, key parsing.
- **TerminalServiceImplPerConnection**: One VirtualTui per StreamTerminalIO call. Daemon with LiveKit exposes TerminalService (per-connection VirtualTui) instead of EchoService.
- **E2E**: two_grpc_clients_get_independent_terminal_streams, two_livekit_clients_get_independent_terminal_streams.
- **Packages**: tddy-core (ViewConnection, NoopView), tddy-tui (VirtualTui), tddy-service (TerminalServiceImplPerConnection, view_connection_factory), tddy-coder (run_daemon wiring), tddy-e2e (spawn helpers, virtual_tui_sessions, terminal_service_livekit).

## 2026-03-14 — Automatic Worktree-per-Workflow

- **Worktree creation**: Each TDD workflow automatically creates a git worktree from `origin/master` (after `git fetch`) after plan approval. Branch and worktree names come from the plan agent's `branch_suggestion` and `worktree_suggestion`.
- **Shared core**: Worktree logic lives in tddy-core; both TUI and daemon use `setup_worktree_for_session`. Daemon no longer uses WorktreeElicitation/ConfirmWorktree — worktree is created automatically after ApprovePlan.
- **Context reminder**: Agent prompts include `repo_dir: <absolute path>` when a worktree is active, so agents know their working directory.
- **Activity pane**: Logs worktree path when the switch happens.
- **Packages**: tddy-core (worktree.rs, workflow/mod.rs, tdd_hooks, workflow_runner, presenter), tddy-service (daemon_service).

## 2026-03-14 — Workflow Restart on Completion

- **Completion behavior**: When a workflow completes successfully with an empty inbox, mode transitions to FeatureInput instead of Done. Users can immediately type a new feature and start another workflow without restarting.
- **Activity log**: Preserved after completion; user can scroll back to previous output.
- **gRPC/daemon**: Clients receive `ModeChanged(FeatureInput)` after WorkflowComplete and can send `SubmitFeatureInput` to start a new workflow.
- **Exit**: Ctrl+C remains the only way to exit from FeatureInput.
- **Packages**: tddy-core (WorkflowComplete handler, SubmitFeatureInput restart, is_done, restart_workflow), tddy-tui (VirtualTui FeatureInput on completion), tddy-e2e (pty/grpc completion assertions), tddy-coder (presenter_integration restart test).

## 2026-03-14 — LiveKit Token Generation

- **CLI args**: `--livekit-api-key` and `--livekit-api-secret` (or `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET` env vars) generate tokens locally instead of requiring pre-generated `--livekit-token`.
- **Mutual exclusivity**: Providing both token and key/secret is an error; one must be chosen.
- **Token refresh**: When using key/secret, tokens auto-refresh by reconnecting 1 minute before expiry. Reconnection loop runs for process lifetime.
- **Modes**: Both daemon and TUI support token generation when key/secret are set.
- **Packages**: tddy-livekit (TokenGenerator, connect_with_bridge, run_with_reconnect), tddy-coder (CLI args, validation, connection paths), tddy-e2e (server_connects_via_token_generator test).

## 2026-03-13 — Dual-Transport Service Codegen

- **tddy-rpc**: New package. Generic RPC framework: Status, Code, Request, Response, Streaming, RpcMessage, RpcService trait, RpcBridge, RpcResult, ResponseBody. Optional tonic feature.
- **tddy-codegen**: Renamed from tddy-livekit-codegen. TddyServiceGenerator generates transport-agnostic service traits, RpcService server structs (per-method handlers, service name validation), tonic adapters (feature-gated).
- **tddy-service**: Renamed from tddy-grpc. Service impls (EchoServiceImpl, TerminalServiceImpl, DaemonService) live here; no transport dependencies.
- **tddy-livekit**: Slimmed to thin LiveKit adapter. Proto envelope, participant, RpcRequest→RpcMessage→RpcBridge. Depends on tddy-rpc only; no service impls.
- **Application layer**: Glues tddy-service + tddy-livekit at runtime (EchoServiceServer + LiveKitParticipant).

## 2026-03-13 — Web Bundle Serving

- **CLI flags**: `--web-port <PORT>` and `--web-bundle-path <PATH>` serve pre-built tddy-web static assets over HTTP. Both flags required together.
- **Modes**: Web server runs in TUI and daemon modes alongside gRPC/LiveKit.
- **Implementation**: axum + tower-http ServeDir; web_server module; validate_web_args for flag validation.
- **Packages**: tddy-coder (web_server.rs, run.rs wiring, acceptance tests).

## 2026-03-13 — LiveKit ConnectRPC Transport for Browser

- **tddy-livekit-web**: New TypeScript package implementing ConnectRPC Transport over LiveKit data channels. Enables browser-based ConnectRPC clients to call unary and streaming RPCs served by Rust LiveKitParticipant.
- **LiveKitTransport**: Implements Transport interface with unary() and stream(); supports unary, client streaming, server streaming, bidirectional streaming.
- **AsyncQueue**: Backpressure-aware async channel for streaming responses.
- **Rust extensions**: tddy-livekit gains EchoClientStream, EchoBidiStream; sender_identity in RpcRequest for targeted response routing; RpcBridge handle_rpc_stream; stream accumulation in LiveKitParticipant.
- **Test infra**: Cypress component tests against real Rust echo server; examples/echo_server.rs; startEchoServer/stopEchoServer/generateToken Cypress tasks.
- **Packages**: tddy-livekit-web (new), tddy-livekit (proto, bridge, client, participant, echo_service, rpc_scenarios).

## 2026-03-12 — Schema via tddy-tools (No Schema Files)

- **Schema ownership**: All schema logic moved to tddy-tools. tddy-core no longer has a schema module, does not write schemas to disk, and does not depend on jsonschema or include_dir.
- **submit**: `tddy-tools submit --goal <goal> --data '<json>'` validates against embedded schemas. No `--schema` file path.
- **get-schema**: New subcommand outputs JSON schema for a goal. Optional `-o <path>` writes to file.
- **Validation error tips**: When validation fails, tddy-tools prints a tip to run `tddy-tools get-schema <goal>`.
- **System prompts**: All goals instruct the agent to use `tddy-tools submit --goal X` and `tddy-tools get-schema X` for format inspection.
- **Packages**: tddy-tools (schemas/, schema.rs, get-schema, --goal), tddy-core (schema module removed, ProcessToolExecutor uses --goal, tdd_hooks no write_schema_to_dir).

## 2026-03-12 — Session Lifecycle Redesign

- **state.session_id**: The `state` section of changeset.yaml includes `session_id` as the single source of truth for the currently-active agent session. Steps read from `state.session_id` instead of tag-based lookups.
- **Early changeset creation**: changeset.yaml is created immediately after the user enters their first prompt, before the workflow starts. Applies to TUI, CLI, and daemon entry paths. The plan dir is resumable even if planning fails.
- **Session capture from first stream event**: When the first system event with `session_id` arrives from the agent stream, the workflow immediately writes the session entry to changeset.sessions and updates `state.session_id`. Session data is persisted within seconds of agent start, not after the step completes.
- **Removed is_resume hack**: Per-step hooks no longer use `context.set_sync("is_resume", true)`. Resume decisions are derived from `state.session_id` in the changeset.
- **Acceptance-tests**: Creates a fresh session (does not resume the plan session). Fixes crash when acceptance-tests tried to resume plan-mode sessions.
- **Green goal**: Reads `state.session_id` from changeset to resume the red session; fallback to tag lookup when state.session_id is absent.
- **Packages**: tddy-core (ChangesetState.session_id, ProgressEvent::SessionStarted, progress_sink with &Context, TddWorkflowHooks SessionStarted handling, early changeset in before_plan), tddy-coder (early changeset in run_plan_to_get_dir), tddy-grpc (early changeset in handle_start_session).

## 2026-03-11 — tddy-tools Submit Only (Drop Inline Parsing)

- **Sole output mechanism**: `tddy-tools submit` via Unix socket is the only way agents deliver structured output. All inline parsing (XML `<structured-response>` blocks, `---PRD_START---`/`---PRD_END---` delimiters, raw JSON prefix checks) has been removed from `output/parser.rs`.
- **Parser simplification**: Each `parse_*_response()` function accepts pre-validated JSON from `tddy-tools submit` and deserializes into typed structs. No text scanning, XML parsing, or delimiter matching.
- **Fail-fast**: When the agent finishes without calling `tddy-tools submit`, the workflow fails immediately with a clear diagnostic (e.g., "Agent finished without calling tddy-tools submit. Ensure tddy-tools is on PATH.").
- **Binary verification**: `tddy-tools` availability is verified at startup before starting any workflow. Fails early if not found.
- **Stream parsing**: Removed `<structured-response>` handling from `stream/mod.rs` and `stream/claude.rs`. Clarification questions still come from `AskUserQuestion` tool events.
- **System prompts**: All goal system prompts (plan, acceptance-tests, red, green, evaluate, validate, refactor, update-docs) instruct the agent to call `tddy-tools submit` with the appropriate schema path.
- **Packages**: tddy-core (parser.rs JSON-only, stream cleanup, fail-fast in PlanTask/BackendInvokeTask), tddy-coder (verify_tddy_tools_available at startup, stub agent option).

## 2026-03-11 — Terminal Streaming via gRPC

- **StreamTerminal RPC**: Server-streaming RPC on TddyRemote service streams raw ANSI bytes from ratatui/crossterm rendering. Clients receive the exact byte stream a terminal would see.
- **CapturingWriter**: tddy-tui captures terminal writes via custom Write implementation; `run_event_loop` accepts optional `ByteCallback`; no-op when not provided.
- **Wiring**: When `--grpc` is set, tddy-coder creates broadcast channel, passes callback to event loop and `TddyRemoteService::with_terminal_bytes`.
- **Use case**: Remote TUI viewer — pipe received bytes into a terminal emulator to render the TUI remotely.
- **Packages**: tddy-tui (CapturingWriter, event_loop byte_capture), tddy-grpc (StreamTerminal proto, service, daemon stub), tddy-coder (run.rs wiring).

## 2026-03-11 — Daemon Mode

- **--daemon flag**: tddy-coder runs as a headless gRPC server for systemd deployment. Process serves multiple sessions sequentially; stateless between sessions (reads changeset.yaml from disk).
- **Session lifecycle**: StartSession creates a new session per prompt. GetSession and ListSessions RPCs query session status from disk. Session states: Pending, Active, WaitingForInput, Completed, Failed.
- **Git worktrees**: Each session gets a worktree in `.worktrees/` (repo root). Worktree path and branch persisted in changeset.yaml. Agent working directory switches to worktree for post-plan steps.
- **Branch/worktree elicitation**: Agent suggests branch and worktree names in plan output; client confirms via WorktreeElicitation. Two-phase flow: PlanApproval then ConfirmWorktree.
- **Commit & push**: Final workflow step instructs agent to commit and push to remote branch. Branch name from changeset context.
- **Packages**: tddy-core (worktree.rs, changeset extensions, ElicitationEvent::WorktreeConfirmation, worktree_dir override, commit/push in tdd_hooks), tddy-grpc (DaemonService, StartSession/ConfirmWorktree flow, proto extensions), tddy-coder (run_daemon, --daemon flag).

## 2026-03-10 — Update-Docs Goal

- **New goal**: `update-docs` runs after refactor as the final workflow step. Reads planning artifacts (PRD.md, progress.md, changeset.yaml, acceptance-tests.md, evaluation-report.md, refactoring-plan.md) and updates target repo documentation per repo guidelines.
- **Workflow**: Full chain is plan → acceptance-tests → red → green → [demo] → evaluate → validate → refactor → update-docs → end.
- **State machine**: `RefactorComplete` → `UpdatingDocs` → `DocsUpdated` (terminal).
- **CLI**: `--goal update-docs --session-dir <path>` accepted by tddy-coder and tddy-demo.
- **CursorBackend**: Supports UpdateDocs (unlike Validate/Refactor which require Agent tool).
- **Schema**: `update-docs.schema.json` with goal, summary, docs_updated.
- **Packages**: tddy-core (workflow/update_docs.rs, parse_update_docs_response, TddWorkflowHooks, tdd_graph), tddy-coder (run.rs value_parser).

## 2026-03-10 — Hook-Triggered Elicitation

- **Orchestrator pause**: Hooks can signal elicitation via `RunnerHooks::elicitation_after_task`. When a hook returns `Some(ElicitationEvent)`, the orchestrator returns `ExecutionStatus::ElicitationNeeded` to the caller instead of auto-continuing to the next task.
- **Plan approval gate fix**: `TddWorkflowHooks` implements elicitation for the plan task (returns `PlanApproval` when PRD.md exists). This fixes the plan approval gate not appearing; previously the orchestrator never returned control between tasks.
- **Caller handling**: `workflow_runner` (TUI) and `run.rs` (plain mode) handle `ElicitationNeeded` in their main loops; present approval UI; resume with user choice. Removed ~400 lines of redundant plan approval loops.
- **Packages**: tddy-core (ElicitationEvent, ExecutionStatus::ElicitationNeeded, RunnerHooks::elicitation_after_task, FlowRunner, WorkflowEngine), tddy-coder (run.rs ElicitationNeeded handlers).

## 2026-03-10 — Stable Session Directory

- **Output location**: Planning output always goes to `$HOME/.tddy/sessions/{uuid}/`. Each session gets a unique UUID subdirectory.
- **Discovery**: Removed `plan_dir_suggestion` from schema; planning prompt uses `name` (human-readable changeset name) instead.
- **Packages**: tddy-core (create_session_dir_in, SESSIONS_SUBDIR, PlanTask session_base), tddy-coder (run.rs output_dir handling).

## 2026-03-10 — Plan Approval Gate

- **Plan approval gate**: After the plan step completes, the user sees a 3-option menu: View (full-screen PRD modal), Approve (proceed to acceptance-tests), or Refine (free-text feedback that resumes the LLM session).
- **Markdown viewer**: Full-screen tui-markdown modal for PRD.md. Keyboard scrolling (Up/Down, PageUp/PageDown). Q or Esc dismisses.
- **Refinement loop**: Refine sends feedback to the plan session; plan re-runs; approval gate re-appears until the user approves.
- **Plain mode**: Text prompt `[v] View  [a] Approve  [r] Refine`; reads choice from stdin.
- **Packages**: tddy-core (WorkflowEvent, AppMode, UserIntent variants; workflow_runner approval loop), tddy-tui (PlanReview/MarkdownViewer rendering, tui-markdown), tddy-coder (plain.rs, run.rs), tddy-grpc (proto intents and modes).

## 2026-03-09 — TUI E2E Testing & Clarification Question Fix

- **tddy-e2e package**: New workspace member for E2E tests. gRPC-driven tests (grpc_clarification, grpc_full_workflow) and PTY test (pty_clarification with termwright, run with `--ignored`).
- **Clarification question rendering**: TUI now displays clarification questions. layout.rs: question_height() for Select/MultiSelect/TextInput. render.rs: render_question (header, options, selection cursor, Other, MultiSelect checkboxes). Dynamic area reuses inbox slot when in question modes.
- **Prompt bar**: Shows "Up/Down navigate Enter select" for Select, "Up/Down navigate Space toggle Enter submit" for MultiSelect, and text input prompt for TextInput/Other modes.
- **Bug fix**: Clarification questions were never visible; root cause was empty prompt bar and missing question widget. Now fully rendered and interactable.

## 2026-03-09 — gRPC Remote Control

- **--grpc option**: tddy-coder and tddy-demo accept `--grpc [PORT]` (e.g. `--grpc 50052`). When provided, starts a tonic gRPC server alongside the TUI. Omit port to use default 50051.
- **Debug area**: Shown only when `--debug` is enabled; hidden otherwise.
- **Bidirectional streaming**: Clients connect via `Stream` RPC; send `UserIntent`s as `ClientMessage`, receive `PresenterEvent`s as `ServerMessage`.
- **tddy-grpc package**: New package with proto definition, TddyRemoteService, conversion layer. Depends on tddy-core.
- **Presenter event bus**: Presenter emits `PresenterEvent`s to optional broadcast channel; gRPC service subscribes and streams to clients.
- **External intents**: TUI event loop drains optional `mpsc::Receiver<UserIntent>`; gRPC forwards client intents to this channel.
- **Use case**: Programmatic control of TUI (e.g., E2E tests, automation) analogous to Selenium for web UIs.

## 2026-03-09 — MVP Architecture Refactoring

- **Presenter** (tddy-core): Owns application state and workflow orchestration. Receives abstract `UserIntent`s (no KeyEvents). Spawns workflow thread; polls `WorkflowEvent`; forwards to `PresenterView` callbacks.
- **tddy-tui** (new package): Ratatui View layer. Implements `PresenterView`; maps crossterm keys to `UserIntent`; holds view-local state (scroll, text buffers, selection cursor); renders activity log, status bar, prompt bar, inbox.
- **tddy-coder**: Removed `tui/` module. Uses Presenter + TuiView + `run_event_loop`. Re-exports presenter types from tddy-core; `disable_raw_mode` from tddy-tui.
- **Integration tests**: Scenario-based `presenter_integration.rs` with TestView + StubBackend. Covers full workflow, clarification round-trip, inbox queue/dequeue, workflow error handling.
- **Done mode**: TUI stays open after workflow completes; user presses Enter or Q to exit. Workflow result printed on exit.
- **User impact**: No change to CLI behavior, TUI layout, or workflow steps.

## 2026-03-09 — Async Workflow Engine with Graph-Flow-Compatible Traits

- **CodingBackend**: Trait is now async; all backends (Claude, Cursor, Mock, Stub) use async invoke.
- **Graph-flow modules**: Task, Context, Graph, FlowRunner, SessionStorage in tddy-core. PlanTask writes PRD.md and TODO.md; BackendInvokeTask for other steps. `build_tdd_workflow_graph()` defines plan→acceptance-tests→red→green→end topology.
- **StubBackend**: New backend for demo and workflow tests. Magic catch-words: CLARIFY, FAIL_PARSE, FAIL_INVOKE. Returns schema-valid structured responses.
- **tddy-demo**: New package — same app as tddy-coder with StubBackend. `--agent stub` only. Self-documenting tutorial.
- **run_plan_via_flow_runner**: FlowRunner-based plan execution; used when migrating CLI/TUI from Workflow to FlowRunner.
- **Backend create-once**: SharedBackend wraps backend; created once per run, reused across goals.

## 2026-03-08 — TDD Workflow Restructure

- **Full workflow**: plan → acceptance-tests → red → green → demo-prompt → evaluate (previously ended at green)
- **Demo step**: Extracted from green into standalone goal; user prompted "Run demo? [r] Run [s] Skip" after green; Skip proceeds to evaluate
- **CLI rename**: `--goal evaluate` replaces `--goal validate-changes`; `--goal demo` added for standalone demo
- **Early changeset**: `changeset.yaml` written immediately after user enters prompt (before plan agent), so plan dir is resumable even if planning fails
- **Single Workflow instance**: Plain full-run uses one Workflow instance throughout (like TUI path)
- **State machine**: `DemoRunning`, `DemoComplete`; `next_goal_for_state`: GreenComplete → demo, DemoComplete → evaluate; when demo skipped, evaluate runs directly from GreenComplete

## 2026-03-08 — TUI UX, Plan Resume, Ctrl+C

- **TUI scroll**: PageUp/PageDown for activity log; no mouse capture so terminal text selection works.
- **Ctrl+C**: Raw mode with ISIG preserved; ctrlc handler restores LeaveAlternateScreen, cursor Show, disable_raw_mode.
- **Plan resume**: When `--session-dir` has Init state and no PRD.md, runs plan() to complete the plan.
- **Debug area**: `--debug` enables TUI debug area and TDDY_QUIET bypass for debug output.

## 2026-03-08 — Agent Inbox

- **Inbox queue**: During Running mode, users type prompts and press Enter to queue them. Queued items display between the activity log and status bar.
- **Navigation**: Up/Down arrows (when input empty) move focus to inbox list; Up/Down navigate items; Esc returns to input.
- **Edit/Delete**: E on selected item enters edit mode (Enter saves, Esc discards); D removes the item.
- **Auto-resume**: On WorkflowComplete with non-empty inbox, the first item is dequeued and sent to the workflow thread. Agent receives an instruction prefix indicating items were queued.
- **Workflow loop**: The workflow thread loops after each cycle; waits for new prompt via channel; exits when channel closes.
- **Layout**: Inbox region has height 0 when empty or not in Running mode.

## 2026-03-08 — TUI with ratatui

- **TUI layout**: Scrollable activity log (top), status bar (middle), prompt bar (bottom). Uses ratatui + crossterm with alternate screen buffer.
- **Status bar**: Displays Goal, State, elapsed time. Goal-specific background colors (plan: yellow, acceptance-tests: orange, red: red, green: green, evaluate/validate: blue). Bold white text. Blank line before status bar.
- **Prompt bar**: Fixed at bottom with "> " prefix. Placeholder when empty: "> Type your feature description and press Enter..."
- **"Other" option**: Select and MultiSelect clarification prompts include "Other (type your own)" as last choice. Selecting it enables free-text input.
- **Piped mode**: When stdin or stderr is not a TTY, TUI is skipped; plain mode uses linear eprintln output.
- **Agent output**: Always visible. On resume (Claude/Cursor --resume) with `--conversation-output`, replayed output is skipped; only new output is echoed.
- **inquire removed**: Replaced entirely by custom ratatui widgets.

## 2026-03-08 — Context Header for Agent Prompts

- **Context reminder**: Plan, acceptance-tests, and red prompts are prepended with a `<context-reminder>` block listing absolute paths to existing .md artifacts (PRD.md, TODO.md, acceptance-tests.md, etc.) when the session directory contains them.
- **Format**: Header starts with `**CRITICAL FOR CONTEXT AND SUMMARY**`; each line is `{filename}: {absolute_path}`. Omitted when plan dir is empty or no .md files exist.
- **Agent awareness**: Agents receive immediate visibility of available plan context files without discovering them.

## 2026-03-08 — Plan Directory Relocation (plan_dir_suggestion)

- **Agent-decided location**: When the plan agent returns `plan_dir_suggestion` in discovery, the workflow relocates the session directory from staging (output_dir) to the suggested path relative to the git root (e.g. `docs/dev/1-WIP/2026-03-08-feature/`).
- **Exit output**: On successful exit, tddy-coder prints the session directory path (plan, acceptance-tests, red, green goals and full workflow).
- **Resume**: Full workflow resume requires `--session-dir`; automatic discovery removed.
- **Validation**: Invalid suggestions (absolute paths, `..`, empty) fall back to staging location. Cross-device moves use copy-then-delete when rename fails.

## 2026-03-08 — JSON Schema Structured Output Validation

- **Schema files**: Formal JSON Schema files for all 7 goals (plan, acceptance-tests, red, green, validate, evaluate, validate-refactor) with shared types via `$ref` in `schemas/common/`.
- **Embedding**: Schemas embedded in binary via `include_dir`; written to `{plan-dir}/schemas/` for agent Read tool.
- **Working directory**: Agent runs with working_dir = plan_dir for plan, acceptance-tests, red, green, validate-refactor so `schemas/xxx.schema.json` resolves to `{plan-dir}/schemas/xxx.schema.json`. Validate and evaluate use working_dir for schema location.
- **Validation**: Agent output validated against schema before serde deserialization. On failure: 1 retry with validation errors and schema path in prompt.
- **Explicit contract**: `<structured-response schema="schemas/red.schema.json">` attribute declares expected format. System prompts reference schema path and include `schema=` in examples.
- **Tests**: Fixtures for valid and invalid JSON per goal; retry integration tests (invalid→valid succeeds; invalid twice→Failed).

## 2026-03-07 — Validate-Changes Goal (removed 2026-03-08, superseded by evaluate)

- **New goal**: `--goal validate-changes` analyzed current git changes for risks (build validity, test infrastructure, production code quality, security). Produced validation-report.md in working directory.
- **Standalone**: Callable from Init without prior plan/red/green. Optional `--session-dir` for changeset/PRD context. Used a fresh session (not resumed).
- **Permission**: validate_allowlist permitted Read, Glob, Grep, SemanticSearch, git diff/log, find, cargo build/check.
- **State**: Init → Validating → Validated. Not in next_goal_for_state auto-sequence.
- **CLI**: `--conversation-output <path>` writes raw agent bytes in real time (each line appended as received).

## 2026-03-07 — Conversation Logging

- **CLI**: `--conversation-output <path>` logs the entire agent conversation in raw bytes to the specified file. Each NDJSON line is written in real time as it is received, so you can tail the file during long runs.

## 2026-03-07 — Backend Abstraction (OCP)

- **Backends**: Claude Code CLI and Cursor agent supported. Use `--agent claude` (default) or `--agent cursor`
- **CLI**: `--agent <name>` selects backend; `--prompt <text>` provides feature description (alternative to stdin)
- **Architecture**: InvokeRequest slimmed (Goal enum, no Claude-specific fields). InvokeResponse.session_id optional. Stream parsing split per backend (stream/claude.rs, stream/cursor.rs)
- **changeset.yaml**: Session entries include `agent` field for resume

## 2026-03-07 — Full Workflow When --goal Omitted

- **Full workflow**: When `--goal` is omitted, tddy-coder runs plan → acceptance-tests → red → green in a single invocation
- **Resume**: Auto-detects completed state from `changeset.yaml`; re-running skips completed steps (via `--session-dir`)
- **CLI**: `--goal` is now optional; individual goals (`plan`, `acceptance-tests`, `red`, `green`) unchanged
- **Output**: Full workflow prints green step output on success; when `GreenComplete`, re-running exits with summary

## 2026-03-10 — Goal Enhancements

- **changeset.yaml**: Replaces `.session` and `.impl-session` as the unified manifest. Contains name (PRD name from plan agent), initial_prompt, clarification_qa, sessions (with system_prompt_file per session), state, models, discovery, artifacts.
- **Plan goal**: Project discovery (toolchain, scripts, doc locations, relevant code). Demo planning (demo-plan.md). Agent decides PRD name. Stores initial_prompt and clarification_qa in changeset.yaml.
- **Observability**: Each goal displays agent and model before execution. State transitions displayed.
- **System prompts**: Stored in session directory (e.g. system-prompt-plan.md); referenced per-session via system_prompt_file in changeset.yaml.
- **Green goal**: Executes demo plan when demo-plan.md exists; writes demo-results.md.
- **Model resolution**: Goals use model from changeset.yaml when --model not specified; CLI --model overrides.

## 2026-03-07 — Green Goal & Implementation Step

- **Green goal**: `--goal green --session-dir <path>` resumes red session via `.impl-session`, implements production code to make failing tests pass, updates progress.md and acceptance-tests.md
- **Red goal**: Now persists session ID to `.impl-session` for green to resume
- **State machine**: New states GreenImplementing, GreenComplete
- **Documentation**: Red and green moved to `implementation-step.md`; `planning-step.md` covers only plan and acceptance-tests
- **CLI**: `--goal green` requires `--session-dir`

## 2026-03-07 — Red Goal & Acceptance-Tests.md

- **Red goal**: `--goal red --session-dir <path>` reads PRD.md and acceptance-tests.md, creates skeleton production code and failing lower-level tests via Claude
- **acceptance-tests.md**: acceptance-tests goal now writes acceptance-tests.md (structured list + rich descriptions) to the session directory
- **State machine**: New states RedTesting, RedTestsReady
- **CLI**: `--goal red` requires `--session-dir`

## 2026-03-07 — Permission Handling in Claude Code Print Mode

- **Print mode constraint**: tddy-coder uses Claude Code in print mode (`-p`); stdin is not used for interactive permission prompts
- **Hybrid policy**: Each goal has a predefined allowlist passed as `--allowedTools`; plan: Read, Glob, Grep, SemanticSearch; acceptance-tests: Read, Write, Edit, Glob, Grep, Bash(cargo *), SemanticSearch
- **CLI**: `--allowed-tools` adds extra tools to the goal allowlist; `--debug` prints Claude CLI command and cwd
- **tddy-permission crate**: MCP server with `approval_prompt` tool for unexpected permission requests (TTY IPC deferred)

## 2026-03-07 — Acceptance Tests Goal

- **New goal**: `--goal acceptance-tests --session-dir <path>` reads a completed plan, resumes the Claude session, creates failing acceptance tests, and verifies they fail
- **Session persistence**: Plan goal now writes `.session` file for session resumption
- **Testing Plan in PRD**: Plan system prompt requires a Testing Plan section (test level, acceptance tests list, target files, assertions)
- **State machine**: New states `AcceptanceTesting` and `AcceptanceTestsReady`
- **CLI**: `--session-dir` flag required for acceptance-tests goal

## 2026-03-07 — Claude Stream-JSON Backend

- **Output format**: Switched from plain text to NDJSON stream (`--output-format=stream-json`)
- **Session management**: `--session-id` on first call, `--resume` on Q&A followup for context continuity
- **Structured Q&A**: Questions from `AskUserQuestion` tool events; TUI mode uses ratatui Select/MultiSelect with "Other" option; plain mode uses stdin (one answer per line)
- **Real-time progress**: Tool activity display (Read, Glob, Bash, etc.)
- **Output parsing**: Structured-response format (`<structured-response content-type="application-json">`) with delimiter fallback
- **Agent output**: Always visible; on resume with `--conversation-output`, replayed output is skipped
