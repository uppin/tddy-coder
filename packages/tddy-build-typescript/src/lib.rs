//! `tddy-build` plugin: lowers `typescript` targets to `bun run <script>`.
//!
//! `typescript` → `bun run <build_script>` (default `build`), run in `package_dir`.

use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction, OutputDecl, OutputKind};
use tddy_build::BuildError;

/// Lowers TypeScript build targets via `bun`.
pub struct TypeScriptPlugin;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TypeScript {
    package_dir: String,
    build_script: String,
    srcs: Vec<String>,
    output_dirs: Vec<String>,
}

impl BuildPlugin for TypeScriptPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["typescript"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let ts: TypeScript = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid typescript config: {e}")))?;

        let script = if ts.build_script.is_empty() {
            "build".to_string()
        } else {
            ts.build_script
        };
        let outputs: Vec<OutputDecl> = ts
            .output_dirs
            .iter()
            .map(|d| OutputDecl {
                path: join_pkg(&ts.package_dir, d),
                kind: OutputKind::Directory as i32,
            })
            .collect();
        Ok(vec![BuildAction {
            id: "typescript".to_string(),
            description: format!("bun run {script}"),
            r#type: ActionType::Command as i32,
            command: vec!["bun".to_string(), "run".to_string(), script],
            inputs: tddy_build::srcs_to_inputs(&ts.srcs, &ts.package_dir),
            outputs,
            working_dir: ts.package_dir,
            ..Default::default()
        }])
    }
}

fn join_pkg(package_dir: &str, dir: &str) -> String {
    if package_dir.is_empty() {
        dir.to_string()
    } else {
        format!("{}/{}", package_dir.trim_end_matches('/'), dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> BuildAction {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "typescript",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let mut actions = TypeScriptPlugin.lower(&ctx).expect("lower");
        assert_eq!(actions.len(), 1);
        actions.remove(0)
    }

    #[test]
    fn typescript_runs_bun_in_package_dir() {
        // Given / When
        let action = lower("package_dir: packages/web\nbuild_script: build\noutput_dirs: [dist]\n");

        // Then
        assert_eq!(action.command, vec!["bun", "run", "build"]);
        assert_eq!(action.working_dir, "packages/web");
    }

    #[test]
    fn typescript_defaults_build_script() {
        // Given / When
        let action = lower("package_dir: packages/web\n");

        // Then
        assert_eq!(action.command, vec!["bun", "run", "build"]);
    }

    #[test]
    fn typescript_emits_srcs_rooted_at_package_dir_and_output_dirs() {
        // Given / When
        let action = lower(
            "package_dir: packages/shared\nbuild_script: build\n\
             srcs: [\"src/index.ts\", \"package.json\"]\noutput_dirs: [dist]\n",
        );

        // Then
        assert_eq!(action.command, vec!["bun", "run", "build"]);
        assert_eq!(action.working_dir, "packages/shared");
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(action.inputs[0].root, "packages/shared");
        assert_eq!(
            action.inputs[0].include,
            vec!["src/index.ts", "package.json"]
        );
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].path, "packages/shared/dist");
        assert_eq!(
            action.outputs[0].kind,
            tddy_build::OutputKind::Directory as i32
        );
    }
}
