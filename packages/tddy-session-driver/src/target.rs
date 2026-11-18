//! Where the driver's session runs: this machine, an ssh `Host` alias, or a Coder workspace.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use tddy_credentials::SecretString;
use tddy_remote_shell::{CoderCredentials, CoderShell, CoderWorkspaceRef, RemoteShell, SshShell};

/// The target `--target` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverTarget {
    /// `local`: the agent runs on this machine, in a PTY of its own.
    Local,
    /// `ssh:<alias>`: the agent runs on the host an ssh `Host` alias names.
    Ssh { alias: String },
    /// `coder:<owner>/<workspace>[.<agent>]`: the agent runs in a Coder workspace.
    Coder { workspace: CoderWorkspaceRef },
}

impl FromStr for DriverTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "local" {
            return Ok(Self::Local);
        }
        if let Some(alias) = value.strip_prefix("ssh:") {
            if alias.is_empty() {
                return Err("ssh: names no Host alias".to_string());
            }
            return Ok(Self::Ssh {
                alias: alias.to_string(),
            });
        }
        if let Some(workspace) = value.strip_prefix("coder:") {
            let (owner, name) = workspace
                .split_once('/')
                .ok_or_else(|| format!("coder:{workspace} is not <owner>/<workspace>[.<agent>]"))?;
            let (name, agent) = match name.split_once('.') {
                Some((name, agent)) => (name, Some(agent)),
                None => (name, None),
            };
            if owner.is_empty() || name.is_empty() || agent.is_some_and(str::is_empty) {
                return Err(format!(
                    "coder:{workspace} is not <owner>/<workspace>[.<agent>]"
                ));
            }
            let mut workspace = CoderWorkspaceRef::new(owner, name);
            if let Some(agent) = agent {
                workspace = workspace.on_agent(agent);
            }
            return Ok(Self::Coder { workspace });
        }
        Err(format!(
            "{value} is not a target: expected local, ssh:<alias> or coder:<owner>/<workspace>[.<agent>]"
        ))
    }
}

impl DriverTarget {
    /// The shell that reaches this target, or `None` for [`Self::Local`]. A Coder target takes its
    /// deployment and token from the CLI's own variables, read through `variable`, and is refused
    /// naming the one that is missing.
    pub fn shell(
        &self,
        coder_binary: &Path,
        variable: impl Fn(&str) -> Option<String>,
    ) -> Result<Option<Arc<dyn RemoteShell>>, String> {
        match self {
            Self::Local => Ok(None),
            Self::Ssh { alias } => Ok(Some(Arc::new(SshShell::new(alias.clone())))),
            Self::Coder { workspace } => {
                let required = |name: &str| {
                    variable(name).filter(|value| !value.is_empty()).ok_or_else(|| {
                        format!(
                            "a Coder target needs {name} in the environment, as the coder CLI reads it"
                        )
                    })
                };
                let url = required(CoderShell::URL_VARIABLE)?;
                let token = required(CoderShell::SESSION_TOKEN_VARIABLE)?;
                Ok(Some(Arc::new(CoderShell::through(
                    coder_binary,
                    CoderCredentials::new(url, SecretString::new(token)),
                    workspace.clone(),
                ))))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn each_target_form_reads_as_its_target() {
        // Given / When
        let read: Vec<Result<DriverTarget, String>> = [
            "local",
            "ssh:buildbox",
            "coder:ada/devbox",
            "coder:ada/devbox.main",
        ]
        .iter()
        .map(|form| form.parse())
        .collect();

        // Then
        assert_eq!(
            read,
            vec![
                Ok(DriverTarget::Local),
                Ok(DriverTarget::Ssh {
                    alias: "buildbox".to_string()
                }),
                Ok(DriverTarget::Coder {
                    workspace: CoderWorkspaceRef::new("ada", "devbox")
                }),
                Ok(DriverTarget::Coder {
                    workspace: CoderWorkspaceRef::new("ada", "devbox").on_agent("main")
                }),
            ]
        );
    }

    #[test]
    fn a_malformed_target_is_refused_naming_the_forms() {
        // Given / When
        let refused: Vec<bool> = [
            "",
            "ssh:",
            "coder:ada",
            "coder:/devbox",
            "coder:ada/devbox.",
            "vm:x",
        ]
        .iter()
        .map(|form| form.parse::<DriverTarget>().is_err())
        .collect();

        // Then
        assert_eq!(refused, vec![true; 6]);
    }

    #[test]
    fn a_coder_target_without_its_token_is_refused_naming_the_variable() {
        // Given
        let target: DriverTarget = "coder:ada/devbox".parse().expect("a Coder target");

        // When
        let refused = target
            .shell(Path::new("coder"), |name| {
                (name == "CODER_URL").then(|| "https://coder.example.com".to_string())
            })
            .map(|_| ());

        // Then
        assert!(
            matches!(&refused, Err(message) if message.contains("CODER_SESSION_TOKEN")),
            "{refused:?}"
        );
    }
}
