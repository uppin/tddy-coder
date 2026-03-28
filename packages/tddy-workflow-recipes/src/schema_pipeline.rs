//! Proto layout and generated artifact paths (PRD F1/F2).
//! Proto basenames are generated into `generated/proto_basenames.rs` from `goals.json` by `build.rs`.

use log::{debug, info};
use std::path::{Path, PathBuf};

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/generated/proto_basenames.rs"
));

/// Root directory for workflow `.proto` files (single source of truth, PRD F1).
pub fn proto_root() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("proto");
    debug!(target: "tddy_workflow_recipes::schema_pipeline", "proto_root");
    info!(
        target: "tddy_workflow_recipes::schema_pipeline",
        "proto_root {}",
        p.display()
    );
    p
}

/// Basenames of `.proto` files for each workflow goal (from `goals.json`, via build script).
pub fn expected_proto_basenames() -> &'static [&'static str] {
    EXPECTED_PROTO_BASENAMES
}

/// Path to generated `schema-manifest.json` (PRD F2).
pub fn generated_manifest_path() -> PathBuf {
    debug!(
        target: "tddy_workflow_recipes::schema_pipeline",
        "generated_manifest_path"
    );
    Path::new(env!("CARGO_MANIFEST_DIR")).join("generated/schema-manifest.json")
}
