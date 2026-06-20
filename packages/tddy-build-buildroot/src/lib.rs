//! `tddy-build` plugin: lowers `buildroot_image` targets to `make`.
//!
//! `buildroot_image` → `make O=<o_rel> <defconfig>` then `make O=<o_rel>`,
//! run in `buildroot_dir`. `o_rel` is `output_dir` expressed relative to `buildroot_dir`
//! so that `make` writes to the correct repo-root-relative location. Output paths in
//! `BuildAction` remain repo-root-relative (as required by the executor).

use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction, FileSet, OutputDecl, OutputKind};
use tddy_build::BuildError;

/// Lowers Buildroot OS image targets via `make`.
pub struct BuildrootPlugin;

impl BuildPlugin for BuildrootPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["buildroot_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let cfg: BuildrootImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid buildroot_image config: {e}")))?;

        if cfg.defconfig.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: defconfig is required".into(),
            ));
        }
        if cfg.buildroot_dir.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: buildroot_dir is required".into(),
            ));
        }
        if cfg.output_dir.is_empty() {
            return Err(BuildError::Manifest(
                "buildroot_image: output_dir is required".into(),
            ));
        }

        let o_rel = relative_path(&cfg.buildroot_dir, &cfg.output_dir);
        let o_arg = format!("O={o_rel}");
        let config_path = format!("{}/.config", cfg.output_dir);

        let final_outputs = if cfg.outputs.is_empty() {
            vec![tddy_build::OutputSpec {
                path: format!("{}/images/rootfs.ext4", cfg.output_dir),
                kind: "file".to_string(),
            }]
        } else {
            cfg.outputs.clone()
        };

        let defconfig_action = BuildAction {
            id: "buildroot-defconfig".to_string(),
            description: format!("make {}", cfg.defconfig),
            r#type: ActionType::Command as i32,
            command: vec!["make".to_string(), o_arg.clone(), cfg.defconfig.clone()],
            inputs: tddy_build::srcs_to_inputs(&cfg.srcs, ""),
            outputs: vec![OutputDecl {
                path: config_path.clone(),
                kind: OutputKind::File as i32,
            }],
            working_dir: cfg.buildroot_dir.clone(),
            ..Default::default()
        };

        let build_action = BuildAction {
            id: "buildroot-build".to_string(),
            description: "make".to_string(),
            r#type: ActionType::Command as i32,
            command: vec!["make".to_string(), o_arg],
            inputs: vec![FileSet {
                include: vec![config_path],
                exclude: Vec::new(),
                root: String::new(),
            }],
            outputs: tddy_build::outputs_to_decls(&final_outputs)?,
            working_dir: cfg.buildroot_dir,
            ..Default::default()
        };

        Ok(vec![defconfig_action, build_action])
    }
}

/// Compute the relative path from `from_dir` to `to_path`, both repo-root-relative.
///
/// Used to pass `O=<rel>` to `make` so it writes to the correct location when
/// running inside `buildroot_dir` rather than the repo root.
fn relative_path(from_dir: &str, to_path: &str) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = to_path.split('/').filter(|s| !s.is_empty()).collect();
    let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();
    let ups = from.len() - common;
    let down = &to[common..];
    let mut parts: Vec<&str> = std::iter::repeat_n("..", ups).collect();
    parts.extend(down.iter().copied());
    if parts.is_empty() { ".".to_string() } else { parts.join("/") }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BuildrootImage {
    defconfig: String,
    buildroot_dir: String,
    output_dir: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> Vec<BuildAction> {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "buildroot_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        BuildrootPlugin.lower(&ctx).expect("lower")
    }

    fn lower_err(fields_yaml: &str) -> BuildError {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "buildroot_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        BuildrootPlugin.lower(&ctx).expect_err("expected error")
    }

    #[test]
    fn defconfig_action_has_correct_argv() {
        // O= is relative from buildroot_dir to output_dir so make writes to the
        // correct repo-root-relative location (external/buildroot/../../build/br-out).
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(
            actions[0].command,
            vec!["make", "O=../../build/br-out", "qemu_x86_64_defconfig"]
        );
        assert_eq!(actions[0].id, "buildroot-defconfig");
        assert_eq!(actions[0].working_dir, "external/buildroot");
    }

    #[test]
    fn build_action_has_correct_argv() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[1].command, vec!["make", "O=../../build/br-out"]);
        assert_eq!(actions[1].id, "buildroot-build");
        assert_eq!(actions[1].working_dir, "external/buildroot");
    }

    #[test]
    fn relative_path_crosses_directory_boundary() {
        assert_eq!(relative_path("external/buildroot", "build/br-out"), "../../build/br-out");
    }

    #[test]
    fn relative_path_nested_under_from() {
        assert_eq!(relative_path("external/br", "external/br/output"), "output");
    }

    #[test]
    fn intermediate_config_wires_defconfig_to_build() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[0].outputs[0].path, "build/br-out/.config");
        assert_eq!(actions[1].inputs[0].include, vec!["build/br-out/.config"]);
    }

    #[test]
    fn inferred_output_defaults_to_rootfs_ext4() {
        let actions = lower("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\n");
        assert_eq!(actions[1].outputs[0].path, "build/br-out/images/rootfs.ext4");
    }

    #[test]
    fn explicit_outputs_override_default() {
        let actions = lower(
            "defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\noutput_dir: build/br-out\noutputs:\n  - path: build/br-out/images/rootfs.img\n    kind: file\n",
        );
        assert_eq!(actions[1].outputs[0].path, "build/br-out/images/rootfs.img");
    }

    #[test]
    fn missing_defconfig_is_rejected() {
        assert!(matches!(
            lower_err("buildroot_dir: external/buildroot\noutput_dir: build/br-out\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn missing_buildroot_dir_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: qemu_x86_64_defconfig\noutput_dir: build/br-out\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn missing_output_dir_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: qemu_x86_64_defconfig\nbuildroot_dir: external/buildroot\n"),
            BuildError::Manifest(_)
        ));
    }

    #[test]
    fn unknown_field_is_rejected() {
        assert!(matches!(
            lower_err("defconfig: x\nbuildroot_dir: d\noutput_dir: o\nbogus: 1\n"),
            BuildError::Manifest(_)
        ));
    }
}
