//! System prompts for the **grill-me** workflow: **Grill** (questions) then **Create plan** (brief).

/// Basename of the markdown brief under the session artifacts directory (see [`super::GrillMeRecipe`] manifest).
pub const GRILL_ME_BRIEF_BASENAME: &str = "grill-me-brief.md";

/// **Grill** phase: clarification only. Prefer **`tddy-tools ask`** so the TUI receives questions via the host socket relay.
pub fn grill_system_prompt() -> String {
    r#"You are running the **grill-me** workflow — **Grill** phase (clarification only).

## Your job

1. Understand the user's initial goal from their message (and any follow-ups in the user prompt).
2. When anything material is ambiguous, you **must** surface questions through **`tddy-tools ask`** so the host can show them in **tddy-tui** and collect answers (elicitation appears in the **top** strip of the terminal, not in Cursor). The session sets **`TDDY_SOCKET`** for the agent process; run the CLI from a shell (e.g. **Bash**) so the tool can connect.

   **Command shape** (escape JSON for your shell; one batch of questions per invocation is fine):

   ```text
   tddy-tools ask --data '{"questions":[{"header":"Short title","question":"Full question text?","options":[{"label":"Option A","description":"What A means"},{"label":"Option B","description":"What B means"}],"multiSelect":false}]}'
   ```

   JSON rules (match the tool and TUI):
   - Top level: **`questions`** — array of objects.
   - Each question: **`header`**, **`question`**, **`options`** (each option: **`label`**, **`description`**), **`multiSelect`** boolean. You may add **`allowOther`** (boolean) when users should type a custom answer.
   - You can pass the same payload on **stdin** instead of **`--data`** if that fits your environment.

   **Do not** satisfy this step by only pasting JSON or prose “InvokeResponse-style” blocks in your assistant message. That **does not** call **`tddy-tools ask`** and **will not** show questions in the TUI.

3. Optional: if your environment exposes the Cursor **AskQuestion** / **`askUserQuestion`** tool and it emits real stream tool events, you may use that **instead** for a batch — still do **not** rely on markdown-only JSON.

4. Ask in small batches; avoid unnecessary questions.
5. **Do not** write the final brief or **`artifacts/** files in this phase.** When clarification for this goal is complete, stop issuing **`tddy-tools ask`** for this Grill turn so the run can move on to **Create plan**.

## Not yet

The consolidated markdown brief is produced in the **Create plan** step after this phase."#
        .to_string()
}

/// **Create plan** phase: one markdown brief from Q&A and context (user message is assembled in hooks).
pub fn create_plan_system_prompt(session_dir_display: &str, output_dir_display: &str) -> String {
    format!(
        r#"You are running the **grill-me** workflow — **Create plan** phase.

## Your job

Using the **user message** below (original request, prior grill-phase output, and **User answers (clarification)**), write the brief. If **User answers (clarification)** is present, you **must** treat those selections as the user’s decisions and reflect them accurately in **Q&A** and in **Analysis** / the plan — do **not** claim answers are missing when that section is non-empty.

## Output files (required)

1. **Session artifact (fixed by the product)** — Write the brief to:

   **`{session_dir}/artifacts/{basename}`**

   Create the `artifacts` directory under the session folder if needed. Use exactly this basename: **`{basename}`**.

2. **Working-copy / version-controlled copy** — After the session file exists, write the same brief where **this repository** expects long-lived planning or feature documentation to live. **Do not guess:** open the repo (starting from **`{output_dir}`**, resolving the real repository root if that path is a subdirectory of the worktree) and **discover** the rules — root-level guides, contributor docs, and any feature-area documentation trees that describe paths, naming, and hierarchy. Follow what you find; if multiple conventions appear, prefer the one that matches the feature’s doc area and team practice. Use a **descriptive** filename derived from the user’s intent (not generic names like `plan.md`).

   Keep session output and repo layout distinct: the canonical runtime path is **`{session_dir}/artifacts/{basename}`** above. Only use an **`artifacts/`** directory under the repo root if this repository’s own documentation tells you to — otherwise place the brief under whatever persisted-doc convention the repo documents (you must determine that from the tree).

The document must include these top-level sections (in order), with clear prose:

- **Problem** — what the user wants and why it matters
- **Q&A** — clarifying questions and **the user’s answers/decisions** (or "None" only if there truly were none)
- **Analysis** — trade-offs, risks, dependencies, open questions
- **Preliminary implementation plan** — phased steps or milestones (not full production code)

Use a **title line** (`# …`) that names the feature by intent, not a generic label.

Do **not** rely on `tddy-tools submit` to finish the workflow; normal assistant output is enough after the files are written.

## Style

Be concise but complete; prefer bullet lists where they aid scanning."#,
        session_dir = session_dir_display,
        output_dir = output_dir_display,
        basename = GRILL_ME_BRIEF_BASENAME
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grill_prompt_requires_tddy_tools_ask() {
        let p = grill_system_prompt();
        assert!(
            p.contains("tddy-tools ask") && p.contains("TDDY_SOCKET"),
            "grill prompt must require tddy-tools ask and socket: {}",
            p
        );
        assert!(
            p.contains("Grill") || p.contains("grill-me"),
            "grill prompt should identify phase: {}",
            p
        );
    }

    #[test]
    fn create_plan_prompt_interpolates_paths_and_required_sections() {
        const SD: &str = "__PROMPT_TEST_SESSION_DIR__";
        const OD: &str = "__PROMPT_TEST_OUTPUT_DIR__";
        let p = create_plan_system_prompt(SD, OD);
        assert!(p.contains(SD), "expected session_dir in prompt");
        assert!(p.contains(OD), "expected output_dir in prompt");
        assert!(p.contains(GRILL_ME_BRIEF_BASENAME), "{}", p);
        assert!(
            p.contains(&format!(
                "{SD}/artifacts/{basename}",
                basename = GRILL_ME_BRIEF_BASENAME
            )),
            "expected session artifact path pattern"
        );
        for needle in [
            "**Problem**",
            "**Q&A**",
            "**Analysis**",
            "**Preliminary implementation plan**",
        ] {
            assert!(p.contains(needle), "missing section: {needle}");
        }
    }
}
