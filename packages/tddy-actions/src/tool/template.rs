//! Rendering a [`RunSection`]'s argv template into the argv a process is spawned with.

use std::collections::BTreeMap;

use super::{ResolvedTool, RunSection, ToolError};

/// The named values an argv template is rendered with (`model`, `session_id`, `resume`, …).
///
/// A parameter is **set** — for an [`super::ArgvItem::When`] — when it is bound to a non-empty
/// value. `binary` is never a parameter here: [`render_argv`] binds it to the resolved tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateParams {
    values: BTreeMap<String, String>,
    lists: BTreeMap<String, Vec<String>>,
}

impl TemplateParams {
    pub fn new() -> Self {
        Self::default()
    }

    /// `self` with `name` bound to `value`.
    #[must_use]
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.values.insert(name.to_string(), value.to_string());
        self
    }

    /// `self` with `name` bound to `value` when it is `Some`, and left unbound otherwise.
    #[must_use]
    pub fn with_opt(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.with(name, value),
            None => self,
        }
    }

    /// `self` with the flag `name` set (`"1"`) when `on`, and left unbound otherwise.
    #[must_use]
    pub fn with_flag(self, name: &str, on: bool) -> Self {
        self.with_opt(name, on.then_some("1"))
    }

    /// `self` with the list parameter `name` bound to `items`, for an [`super::ArgvItem::Splice`].
    #[must_use]
    pub fn with_list(mut self, name: &str, items: Vec<String>) -> Self {
        self.lists.insert(name.to_string(), items);
        self
    }

    /// The list `name` is bound to.
    pub fn list(&self, name: &str) -> Option<&[String]> {
        self.lists.get(name).map(Vec::as_slice)
    }

    /// The value `name` is bound to.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

/// `run`'s argv for `tool`, rendered with `params`: every literal is one entry with its `{name}`
/// placeholders replaced, `{binary}` by `tool.binary`; a `When` contributes its `then` items when
/// its parameter is set and its `else` items otherwise, recursively; a `Splice` contributes its
/// list's elements.
///
/// An unbound placeholder or list is [`ToolError::UnboundPlaceholder`] naming it — never an empty
/// entry.
pub fn render_argv(
    run: &RunSection,
    tool: &ResolvedTool,
    params: &TemplateParams,
) -> Result<Vec<String>, ToolError> {
    // TODO(claude-host): implement — walk `run.argv`, substitute `{name}` in each literal.
    let _ = (run, tool, params);
    todo!("claude-host: render_argv")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tool::{ArgvItem, Transport};

    fn lit(s: &str) -> ArgvItem {
        ArgvItem::Lit(s.to_string())
    }

    fn when(param: &str, then: Vec<ArgvItem>, else_: Vec<ArgvItem>) -> ArgvItem {
        ArgvItem::When {
            if_: param.to_string(),
            then,
            else_,
        }
    }

    fn a_pty_run(argv: Vec<ArgvItem>) -> RunSection {
        RunSection {
            transport: Transport::Pty,
            argv,
            env: BTreeMap::new(),
        }
    }

    fn a_resolved_tool(binary: &str) -> ResolvedTool {
        ResolvedTool {
            id: "claude".to_string(),
            command: "claude".to_string(),
            binary: PathBuf::from(binary),
            reads: Vec::new(),
            network: Default::default(),
        }
    }

    #[test]
    fn literals_render_one_argv_entry_each_in_order() {
        // Given
        let run = a_pty_run(vec![lit("--print"), lit("--verbose")]);

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &TemplateParams::new());

        // Then
        assert_eq!(
            argv,
            Ok(vec!["--print".to_string(), "--verbose".to_string()])
        );
    }

    #[test]
    fn a_placeholder_inside_a_literal_is_replaced_by_its_value() {
        // Given
        let run = a_pty_run(vec![lit("--model={model}"), lit("{session_id}")]);
        let params = TemplateParams::new()
            .with("model", "opus")
            .with("session_id", "0190-a");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(
            argv,
            Ok(vec!["--model=opus".to_string(), "0190-a".to_string()])
        );
    }

    #[test]
    fn binary_is_the_resolved_tools_canonical_path() {
        // Given
        let run = a_pty_run(vec![lit("{binary}")]);

        // When
        let argv = render_argv(
            &run,
            &a_resolved_tool("/opt/homebrew/Cellar/claude/2.1/bin/claude"),
            &TemplateParams::new(),
        );

        // Then
        assert_eq!(
            argv,
            Ok(vec![
                "/opt/homebrew/Cellar/claude/2.1/bin/claude".to_string()
            ])
        );
    }

    #[test]
    fn a_when_whose_parameter_is_set_renders_its_then_items() {
        // Given
        let run = a_pty_run(vec![when(
            "resume",
            vec![lit("--resume"), lit("{session_id}")],
            vec![lit("--session-id"), lit("{session_id}")],
        )]);
        let params = TemplateParams::new()
            .with_flag("resume", true)
            .with("session_id", "0190-a");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(argv, Ok(vec!["--resume".to_string(), "0190-a".to_string()]));
    }

    #[test]
    fn a_when_whose_parameter_is_unset_renders_its_else_items() {
        // Given
        let run = a_pty_run(vec![when(
            "resume",
            vec![lit("--resume"), lit("{session_id}")],
            vec![lit("--session-id"), lit("{session_id}")],
        )]);
        let params = TemplateParams::new()
            .with_flag("resume", false)
            .with("session_id", "0190-a");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(
            argv,
            Ok(vec!["--session-id".to_string(), "0190-a".to_string()])
        );
    }

    #[test]
    fn a_parameter_bound_to_an_empty_value_is_unset_for_a_when() {
        // Given
        let run = a_pty_run(vec![
            lit("{binary}"),
            when("model", vec![lit("--model"), lit("{model}")], vec![]),
        ]);
        let params = TemplateParams::new().with("model", "");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(argv, Ok(vec!["/bin/agent".to_string()]));
    }

    #[test]
    fn a_nested_when_renders_only_along_the_branches_taken() {
        // Given
        let run = a_pty_run(vec![when(
            "prompt",
            vec![when(
                "print",
                vec![lit("--print"), lit("{prompt}")],
                vec![lit("{prompt}")],
            )],
            vec![lit("--interactive")],
        )]);
        let params = TemplateParams::new().with("prompt", "fix the build");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(argv, Ok(vec!["fix the build".to_string()]));
    }

    #[test]
    fn a_splice_expands_its_list_in_place_one_entry_per_element() {
        // Given
        let run = a_pty_run(vec![
            lit("{binary}"),
            ArgvItem::Splice {
                splice: "slots".to_string(),
            },
            lit("{prompt}"),
        ]);
        let params = TemplateParams::new()
            .with_list(
                "slots",
                vec!["--allowedTools".to_string(), "Read".to_string()],
            )
            .with("prompt", "hello");

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &params);

        // Then
        assert_eq!(
            argv,
            Ok(vec![
                "/bin/agent".to_string(),
                "--allowedTools".to_string(),
                "Read".to_string(),
                "hello".to_string(),
            ])
        );
    }

    #[test]
    fn an_unbound_placeholder_is_an_error_naming_it() {
        // Given
        let run = a_pty_run(vec![
            lit("{binary}"),
            lit("--permission-mode"),
            lit("{permission_mode}"),
        ]);

        // When
        let argv = render_argv(&run, &a_resolved_tool("/bin/agent"), &TemplateParams::new());

        // Then
        assert_eq!(
            argv,
            Err(ToolError::UnboundPlaceholder {
                tool: "claude".to_string(),
                name: "permission_mode".to_string(),
            })
        );
    }
}
