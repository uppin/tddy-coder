//! prost-build: compiles `proto/` into `$OUT_DIR/tddy.build.v1.rs`.
//!
//! Hard Requirement #2 — BUILD.yaml deserializes *directly* into the generated
//! proto types. serde derives are attached to every generated message via
//! `type_attribute(".")`; per-message `default` + `deny_unknown_fields`, the
//! internally-tagged `BuildTarget.config` oneof, and string↔i32 enum converters
//! complete the mapping.

fn main() {
    let mut cfg = prost_build::Config::new();

    // Attach serde derives to every generated message and oneof/enum type.
    cfg.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    // Messages: tolerate omitted fields, reject unknown keys.
    // (The seven `config` variant structs are handled below — they must NOT carry
    // `deny_unknown_fields` because the internal tag key flows into them.)
    for msg in [
        "BuildManifest",
        "BuildTarget",
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

    // Target config variant structs: defaults only (tag key passes through them).
    for msg in [
        "RustBinaryTarget",
        "RustLibraryTarget",
        "TypeScriptTarget",
        "DockerImageTarget",
        "ScriptTarget",
        "ToolTarget",
        "TargetGroupTarget",
    ] {
        cfg.type_attribute(format!("tddy.build.v1.{msg}"), "#[serde(default)]");
    }

    // The `config` oneof: internally tagged by `type`, snake_case variant names.
    cfg.type_attribute(
        "tddy.build.v1.BuildTarget.config",
        "#[serde(tag = \"type\", rename_all = \"snake_case\")]",
    );

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
            "proto/tddy/build/v1/manifest.proto",
            "proto/tddy/build/v1/targets.proto",
            "proto/tddy/build/v1/actions.proto",
            "proto/tddy/build/v1/cache.proto",
        ],
        &["proto"],
    )
    .expect("prost-build failed");

    println!("cargo:rerun-if-changed=proto");
}
