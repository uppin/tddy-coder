//! Path templates: `{name}` placeholders over a fixed vocabulary of variables.

use serde::{Deserialize, Serialize};

/// A path with `{identifier}` placeholders. Braces that do not enclose an identifier stay literal,
/// as in `tddy-actions`' argv templates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathTemplate(String);

/// Why a template could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// The template names a variable its position does not bind.
    UnknownVariable { template: String, variable: String },
    /// The variable is known but has no value for this session.
    Unset { template: String, variable: String },
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::UnknownVariable { template, variable } => {
                write!(
                    f,
                    "template '{template}' names unknown variable '{{{variable}}}'"
                )
            }
            TemplateError::Unset { template, variable } => {
                write!(
                    f,
                    "template '{template}' needs '{{{variable}}}', which is unset"
                )
            }
        }
    }
}

impl std::error::Error for TemplateError {}

impl PathTemplate {
    /// A template over `text`.
    pub fn new(text: impl Into<String>) -> Self {
        PathTemplate(text.into())
    }

    /// The template as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Every `{identifier}` the template names, in order of appearance.
    pub fn variables(&self) -> Vec<&str> {
        // TODO(claude-transcript): implement
        Vec::new()
    }

    /// The template with each variable replaced by `lookup(name)`. A name `allowed` does not hold
    /// is [`TemplateError::UnknownVariable`]; an allowed name `lookup` has no value for is
    /// [`TemplateError::Unset`].
    pub fn render(
        &self,
        allowed: &[&str],
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> Result<String, TemplateError> {
        // TODO(claude-transcript): implement
        let _ = (allowed, lookup);
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWED: &[&str] = &["agent_home", "conversation_id"];

    fn values(name: &str) -> Option<String> {
        match name {
            "agent_home" => Some("/home/a".to_string()),
            _ => None,
        }
    }

    #[test]
    fn each_variable_is_replaced_by_its_value() {
        let template = PathTemplate::new("{agent_home}/.state/{agent_home}.log");

        let rendered = template.render(ALLOWED, &values);

        assert_eq!(rendered, Ok("/home/a/.state//home/a.log".to_string()));
    }

    #[test]
    fn a_variable_outside_the_vocabulary_is_unknown() {
        let template = PathTemplate::new("{agent_home}/{nope}.jsonl");

        let rendered = template.render(ALLOWED, &values);

        assert_eq!(
            rendered,
            Err(TemplateError::UnknownVariable {
                template: "{agent_home}/{nope}.jsonl".to_string(),
                variable: "nope".to_string(),
            })
        );
    }

    #[test]
    fn a_known_variable_without_a_value_is_unset() {
        let template = PathTemplate::new("{agent_home}/{conversation_id}.jsonl");

        let rendered = template.render(ALLOWED, &values);

        assert_eq!(
            rendered,
            Err(TemplateError::Unset {
                template: "{agent_home}/{conversation_id}.jsonl".to_string(),
                variable: "conversation_id".to_string(),
            })
        );
    }

    #[test]
    fn braces_that_enclose_no_identifier_stay_literal() {
        let template = PathTemplate::new("{agent_home}/{a-b}/{}/*.jsonl");

        assert_eq!(
            (template.variables(), template.render(ALLOWED, &values)),
            (
                vec!["agent_home"],
                Ok("/home/a/{a-b}/{}/*.jsonl".to_string())
            )
        );
    }
}
