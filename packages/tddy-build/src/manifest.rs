//! YAML → proto deserialization for `BUILD.yaml` manifests.

use crate::error::BuildError;
use crate::proto::BuildManifest;

/// Deserialize a `BUILD.yaml` document directly into a [`BuildManifest`] proto.
///
/// Hard Requirement #2: YAML maps straight onto the prost-generated types — there
/// is no parallel serde struct layer.
pub fn load_build_manifest(yaml: &str) -> Result<BuildManifest, BuildError> {
    serde_yaml::from_str(yaml).map_err(|e| BuildError::Yaml(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::load_build_manifest;
    use crate::error::BuildError;
    use crate::proto::build_target::Config;

    #[test]
    fn omitted_optional_fields_default_rather_than_erroring() {
        // Only `id` + the typed config are provided; deps/name/actions default.
        let manifest = load_build_manifest(
            "schema_version: 1\ntargets:\n  - id: \"a:bin\"\n    config:\n      type: rust_binary\n      package: a\n",
        )
        .expect("minimal manifest must parse via serde defaults");
        let target = &manifest.targets[0];
        assert_eq!(target.name, "");
        assert!(target.deps.is_empty());
        match target.config.as_ref().unwrap() {
            Config::RustBinary(rb) => {
                assert_eq!(rb.package, "a");
                assert_eq!(rb.profile, "");
            }
            _ => panic!("expected rust_binary"),
        }
    }

    #[test]
    fn unknown_config_variant_tag_is_rejected() {
        let err = load_build_manifest(
            "schema_version: 1\ntargets:\n  - id: x\n    config:\n      type: nonsense\n",
        )
        .expect_err("unknown oneof tag must error");
        assert!(matches!(err, BuildError::Yaml(_)));
    }

    #[test]
    fn explicit_actions_parse_with_enum_strings() {
        let manifest = load_build_manifest(
            "schema_version: 1\ntargets:\n  - id: t\n    actions:\n      - id: step\n        type: command\n        command: [echo, hi]\n        outputs:\n          - path: out.txt\n            kind: file\n",
        )
        .expect("explicit actions must parse");
        let action = &manifest.targets[0].actions[0];
        assert_eq!(action.command, vec!["echo".to_string(), "hi".to_string()]);
        assert_eq!(action.outputs[0].path, "out.txt");
    }
}
