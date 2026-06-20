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
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
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
        let outputs = tddy_build::outputs_to_decls(&d.outputs)?;
        if let Some(first) = outputs.first() {
            command.push("--iidfile".to_string());
            command.push(first.path.clone());
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
            inputs: tddy_build::srcs_to_inputs(&d.srcs, ""),
            outputs,
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
        // Given / When
        let command =
            lower("dockerfile: Dockerfile\ncontext: ctx\ntag: img:latest\nbuild_args: [\"A=1\"]\n");

        // Then
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
        // Given / When
        let command = lower("tag: img:latest\n");

        // Then
        assert_eq!(command, vec!["docker", "build", "-t", "img:latest", "."]);
    }

    #[test]
    fn docker_emits_iidfile_inputs_and_outputs() {
        // Given
        let config: serde_yaml::Value = serde_yaml::from_str(
            "tag: example-base\ndockerfile: base/Dockerfile\ncontext: base\n\
             srcs: [\"base/Dockerfile\"]\n\
             outputs:\n  - path: \".tddy-build/iid/base.txt\"\n    kind: file\n",
        )
        .expect("valid yaml");
        let ctx = LowerContext {
            type_name: "docker_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };

        // When
        let action = DockerPlugin.lower(&ctx).expect("lower").remove(0);

        // Then
        assert_eq!(
            action.command,
            vec![
                "docker",
                "build",
                "-f",
                "base/Dockerfile",
                "-t",
                "example-base",
                "--iidfile",
                ".tddy-build/iid/base.txt",
                "base"
            ]
        );
        assert_eq!(action.inputs[0].include, vec!["base/Dockerfile"]);
        assert_eq!(action.outputs[0].path, ".tddy-build/iid/base.txt");
    }
}
