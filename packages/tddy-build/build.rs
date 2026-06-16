//! prost-build: compiles the action + cache protos into `$OUT_DIR/tddy.build.v1.rs`.
//!
//! Only `BuildAction` (the engine↔plugin contract) and the cache types are proto.
//! The `BUILD.yaml` manifest schema (`BuildManifest`/`BuildTarget`/`TargetConfig`)
//! lives in `src/manifest.rs` as open serde structs so target `config` is plugin-
//! extensible. serde derives are attached to the generated messages; per-message
//! `default` + `deny_unknown_fields` and string↔i32 enum converters complete the
//! mapping for `BuildAction` fields authored in `BUILD.yaml`.

fn main() {
    let mut cfg = prost_build::Config::new();

    // Attach serde derives to every generated message and enum type.
    cfg.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    // Messages: tolerate omitted fields, reject unknown keys.
    for msg in [
        "BuildAction",
        "FileSet",
        "OutputDecl",
        "ActionCacheEntry",
        "FileFingerprint",
    ] {
        cfg.type_attribute(
            format!("tddy.build.v1.{msg}"),
            "#[serde(default, deny_unknown_fields)]",
        );
    }

    // Enum fields authored as snake_case strings ↔ prost `i32`.
    cfg.field_attribute(
        "tddy.build.v1.BuildAction.type",
        "#[serde(serialize_with = \"crate::serde_helpers::serialize_action_type\", \
         deserialize_with = \"crate::serde_helpers::deserialize_action_type\")]",
    );
    cfg.field_attribute(
        "tddy.build.v1.OutputDecl.kind",
        "#[serde(serialize_with = \"crate::serde_helpers::serialize_output_kind\", \
         deserialize_with = \"crate::serde_helpers::deserialize_output_kind\")]",
    );

    cfg.compile_protos(
        &[
            "proto/tddy/build/v1/actions.proto",
            "proto/tddy/build/v1/cache.proto",
        ],
        &["proto"],
    )
    .expect("prost-build failed");

    println!("cargo:rerun-if-changed=proto");
}
