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
