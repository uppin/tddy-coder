//! `tddy-build` plugin: lowers `docker_image` targets to `docker build`.
//!
//! `docker_image` → `docker build [-f <dockerfile>] [-t <tag>] [--build-arg …] <context>`.

use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction};
use tddy_build::BuildError;

/// Lowers Docker image build targets via `docker`.
pub struct DockerPlugin;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DockerImage {
    dockerfile: String,
    context: String,
    tag: String,
    build_args: Vec<String>,
}

impl BuildPlugin for DockerPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["docker_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let d: DockerImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid docker_image config: {e}")))?;

        let description = format!("docker build {}", d.tag);
        let mut command = vec!["docker".to_string(), "build".to_string()];
        if !d.dockerfile.is_empty() {
            command.push("-f".to_string());
            command.push(d.dockerfile);
        }
        if !d.tag.is_empty() {
            command.push("-t".to_string());
            command.push(d.tag);
        }
        for arg in d.build_args {
            command.push("--build-arg".to_string());
            command.push(arg);
        }
        command.push(if d.context.is_empty() {
            ".".to_string()
        } else {
            d.context
        });

        Ok(vec![BuildAction {
            id: "docker-image".to_string(),
            description,
            r#type: ActionType::Command as i32,
            command,
            ..Default::default()
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> Vec<String> {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "docker_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let mut actions = DockerPlugin.lower(&ctx).expect("lower");
        assert_eq!(actions.len(), 1);
        actions.remove(0).command
    }

    #[test]
    fn docker_builds_with_dockerfile_tag_args_and_context() {
        let command =
            lower("dockerfile: Dockerfile\ncontext: ctx\ntag: img:latest\nbuild_args: [\"A=1\"]\n");
        assert_eq!(
            command,
            vec![
                "docker",
                "build",
                "-f",
                "Dockerfile",
                "-t",
                "img:latest",
                "--build-arg",
                "A=1",
                "ctx",
            ]
        );
    }

    #[test]
    fn docker_defaults_context_to_current_dir() {
        let command = lower("tag: img:latest\n");
        assert_eq!(command, vec!["docker", "build", "-t", "img:latest", "."]);
    }
}
