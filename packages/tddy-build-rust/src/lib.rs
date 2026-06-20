//! `tddy-build` plugin: lowers `rust_binary` / `rust_library` targets to `cargo build`.
//!
//! - `rust_binary`  → `cargo build -p <pkg> [--bin <name>] [--features …] [--release] [--target <triple>]`
//! - `rust_library` → `cargo build -p <pkg> [--features …] [--release]`

use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction};
use tddy_build::BuildError;

/// Lowers Rust build targets via `cargo`.
pub struct RustPlugin;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RustBinary {
    package: String,
    bin_name: String,
    features: Vec<String>,
    profile: String,
    target_triple: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RustLibrary {
    package: String,
    features: Vec<String>,
    profile: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
}

impl BuildPlugin for RustPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["rust_binary", "rust_library"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let action = match ctx.type_name {
            "rust_binary" => {
                let rb: RustBinary = parse(ctx)?;
                rust_binary_action(rb)?
            }
            "rust_library" => {
                let rl: RustLibrary = parse(ctx)?;
                rust_library_action(rl)?
            }
            other => {
                return Err(BuildError::Manifest(format!(
                    "tddy-build-rust does not handle target type {other}"
                )))
            }
        };
        Ok(vec![action])
    }
}

fn parse<T>(ctx: &LowerContext) -> Result<T, BuildError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_yaml::from_value(ctx.config.clone())
        .map_err(|e| BuildError::Manifest(format!("invalid {} config: {e}", ctx.type_name)))
}

fn rust_binary_action(rb: RustBinary) -> Result<BuildAction, BuildError> {
    let description = format!("cargo build {}", rb.package);
    let mut command = vec![
        "cargo".to_string(),
        "build".to_string(),
        "-p".to_string(),
        rb.package,
    ];
    if !rb.bin_name.is_empty() {
        command.push("--bin".to_string());
        command.push(rb.bin_name);
    }
    push_features(&mut command, rb.features);
    push_profile(&mut command, &rb.profile);
    if !rb.target_triple.is_empty() {
        command.push("--target".to_string());
        command.push(rb.target_triple);
    }
    finish(
        "rust-binary",
        description,
        command,
        rb.srcs,
        rb.outputs,
        rb.working_dir,
    )
}

fn rust_library_action(rl: RustLibrary) -> Result<BuildAction, BuildError> {
    let description = format!("cargo build {}", rl.package);
    let mut command = vec![
        "cargo".to_string(),
        "build".to_string(),
        "-p".to_string(),
        rl.package,
    ];
    push_features(&mut command, rl.features);
    push_profile(&mut command, &rl.profile);
    finish(
        "rust-library",
        description,
        command,
        rl.srcs,
        rl.outputs,
        rl.working_dir,
    )
}

fn push_features(command: &mut Vec<String>, features: Vec<String>) {
    if !features.is_empty() {
        command.push("--features".to_string());
        command.push(features.join(","));
    }
}

fn push_profile(command: &mut Vec<String>, profile: &str) {
    if profile == "release" {
        command.push("--release".to_string());
    }
}

fn finish(
    id: &str,
    description: String,
    command: Vec<String>,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
    working_dir: String,
) -> Result<BuildAction, BuildError> {
    Ok(BuildAction {
        id: id.to_string(),
        description,
        r#type: ActionType::Command as i32,
        command,
        inputs: tddy_build::srcs_to_inputs(&srcs, ""),
        outputs: tddy_build::outputs_to_decls(&outputs)?,
        working_dir,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(type_name: &str, fields_yaml: &str) -> Vec<String> {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name,
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let mut actions = RustPlugin.lower(&ctx).expect("lower");
        assert_eq!(actions.len(), 1);
        actions.remove(0).command
    }

    #[test]
    fn rust_binary_builds_cargo_argv_with_features_profile_and_triple() {
        // Given / When
        let command = lower(
            "rust_binary",
            "package: app\nbin_name: app\nfeatures: [a, b]\nprofile: release\ntarget_triple: x86_64-unknown-linux-gnu\n",
        );

        // Then
        assert_eq!(
            command,
            vec![
                "cargo",
                "build",
                "-p",
                "app",
                "--bin",
                "app",
                "--features",
                "a,b",
                "--release",
                "--target",
                "x86_64-unknown-linux-gnu",
            ]
        );
    }

    #[test]
    fn rust_library_omits_release_for_debug_profile() {
        // Given / When
        let command = lower("rust_library", "package: core\nprofile: debug\n");

        // Then
        assert_eq!(command, vec!["cargo", "build", "-p", "core"]);
    }

    #[test]
    fn rust_binary_omits_bin_features_and_triple_when_absent() {
        // Given / When
        let command = lower("rust_binary", "package: app\nprofile: debug\n");

        // Then
        assert_eq!(command, vec!["cargo", "build", "-p", "app"]);
    }

    #[test]
    fn unknown_field_in_config_is_rejected() {
        // Given
        let config: serde_yaml::Value = serde_yaml::from_str("package: app\nbogus: 1\n").unwrap();
        let ctx = LowerContext {
            type_name: "rust_binary",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };

        // When / Then
        assert!(matches!(
            RustPlugin.lower(&ctx),
            Err(BuildError::Manifest(_))
        ));
    }

    #[test]
    fn rust_binary_emits_declared_inputs_and_outputs() {
        // Given
        let config: serde_yaml::Value = serde_yaml::from_str(
            "package: mathapp\nbin_name: mathapp\nprofile: debug\n\
             srcs: [\"mathapp/src/main.rs\", \"mathapp/Cargo.toml\"]\n\
             outputs:\n  - path: \"target/debug/mathapp\"\n    kind: file\n",
        )
        .expect("valid yaml");
        let ctx = LowerContext {
            type_name: "rust_binary",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };

        // When
        let actions = RustPlugin.lower(&ctx).expect("lower");
        let action = &actions[0];

        // Then
        assert_eq!(
            action.command,
            vec!["cargo", "build", "-p", "mathapp", "--bin", "mathapp"]
        );
        assert_eq!(action.inputs.len(), 1);
        assert_eq!(
            action.inputs[0].include,
            vec!["mathapp/src/main.rs", "mathapp/Cargo.toml"]
        );
        assert_eq!(action.outputs.len(), 1);
        assert_eq!(action.outputs[0].path, "target/debug/mathapp");
    }
}
