//! `tddy-build` plugin: lowers `qemu_disk_image` targets to `qemu-img convert`.
//!
//! `qemu_disk_image` → `qemu-img convert -f <input_format> -O qcow2 <input> <output>`,
//! run from the repo root. Output path inferred by swapping the input extension to `.qcow2`
//! when not explicitly specified.

use serde::Deserialize;

use tddy_build::plugin::{BuildPlugin, LowerContext};
use tddy_build::proto::{ActionType, BuildAction};
use tddy_build::BuildError;

/// Lowers QEMU disk image targets via `qemu-img convert`.
pub struct QemuPlugin;

impl BuildPlugin for QemuPlugin {
    fn type_names(&self) -> &'static [&'static str] {
        &["qemu_disk_image"]
    }

    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError> {
        let cfg: QemuDiskImage = serde_yaml::from_value(ctx.config.clone())
            .map_err(|e| BuildError::Manifest(format!("invalid qemu_disk_image config: {e}")))?;

        if cfg.input.is_empty() {
            return Err(BuildError::Manifest(
                "qemu_disk_image: input is required".into(),
            ));
        }

        let input_format = if cfg.input_format.is_empty() {
            "raw".to_string()
        } else {
            cfg.input_format.clone()
        };

        let output_path = if cfg.outputs.is_empty() {
            infer_qcow2_path(&cfg.input)?
        } else {
            cfg.outputs[0].path.clone()
        };

        let final_outputs = if cfg.outputs.is_empty() {
            vec![tddy_build::OutputSpec {
                path: output_path.clone(),
                kind: "file".to_string(),
            }]
        } else {
            cfg.outputs.clone()
        };

        Ok(vec![BuildAction {
            id: "qemu-disk-image".to_string(),
            description: format!("qemu-img convert {}", cfg.input),
            r#type: ActionType::Command as i32,
            command: vec![
                "qemu-img".to_string(),
                "convert".to_string(),
                "-f".to_string(),
                input_format,
                "-O".to_string(),
                "qcow2".to_string(),
                cfg.input.clone(),
                output_path,
            ],
            inputs: tddy_build::srcs_to_inputs(&cfg.srcs, ""),
            outputs: tddy_build::outputs_to_decls(&final_outputs)?,
            ..Default::default()
        }])
    }
}

fn infer_qcow2_path(input: &str) -> Result<String, BuildError> {
    let p = std::path::Path::new(input);
    let stem = p
        .file_stem()
        .ok_or_else(|| {
            BuildError::Manifest(format!(
                "qemu_disk_image: cannot infer output path from input {input:?}"
            ))
        })?
        .to_string_lossy();
    match p.parent() {
        Some(parent) if parent != std::path::Path::new("") => {
            Ok(format!("{}/{}.qcow2", parent.to_string_lossy(), stem))
        }
        _ => Ok(format!("{stem}.qcow2")),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct QemuDiskImage {
    input: String,
    input_format: String,
    srcs: Vec<String>,
    outputs: Vec<tddy_build::OutputSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(fields_yaml: &str) -> BuildAction {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "qemu_disk_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        let mut actions = QemuPlugin.lower(&ctx).expect("lower");
        assert_eq!(actions.len(), 1);
        actions.remove(0)
    }

    fn lower_err(fields_yaml: &str) -> BuildError {
        let config: serde_yaml::Value = serde_yaml::from_str(fields_yaml).expect("valid yaml");
        let ctx = LowerContext {
            type_name: "qemu_disk_image",
            target_id: "t",
            target_name: "",
            deps: &[],
            config: &config,
        };
        QemuPlugin.lower(&ctx).expect_err("expected error")
    }

    #[test]
    fn convert_action_has_correct_argv() {
        // Given / When
        let action = lower("input: build/br-out/images/rootfs.ext4\n");

        // Then
        assert_eq!(
            action.command,
            vec![
                "qemu-img",
                "convert",
                "-f",
                "raw",
                "-O",
                "qcow2",
                "build/br-out/images/rootfs.ext4",
                "build/br-out/images/rootfs.qcow2",
            ]
        );
        assert_eq!(action.id, "qemu-disk-image");
    }

    #[test]
    fn inferred_output_swaps_extension_to_qcow2() {
        // Given / When
        let action = lower("input: build/br-out/images/rootfs.ext4\n");

        // Then
        assert_eq!(action.outputs[0].path, "build/br-out/images/rootfs.qcow2");
    }

    #[test]
    fn explicit_outputs_override_default() {
        // Given / When
        let action = lower(
            "input: build/br-out/images/rootfs.ext4\noutputs:\n  - path: build/my-os.qcow2\n    kind: file\n",
        );

        // Then
        assert_eq!(action.outputs[0].path, "build/my-os.qcow2");
        assert_eq!(action.command.last().unwrap(), "build/my-os.qcow2");
    }

    #[test]
    fn custom_input_format_is_used() {
        // Given / When
        let action = lower("input: build/rootfs.qcow2\ninput_format: qcow2\n");

        // Then
        assert_eq!(action.command[3], "qcow2");
    }

    #[test]
    fn default_input_format_is_raw() {
        // Given / When
        let action = lower("input: build/rootfs.ext4\n");

        // Then
        assert_eq!(action.command[3], "raw");
    }

    #[test]
    fn missing_input_is_rejected() {
        // Given / When / Then
        assert!(matches!(lower_err("\n"), BuildError::Manifest(_)));
    }

    #[test]
    fn unknown_field_is_rejected() {
        // Given / When / Then
        assert!(matches!(
            lower_err("input: x.ext4\nbogus: 1\n"),
            BuildError::Manifest(_)
        ));
    }
}
