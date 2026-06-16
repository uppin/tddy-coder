//! tddy-build: Bazel-inspired, content-addressed build system.
//!
//! `BUILD.yaml` manifests deserialize directly into the prost-generated proto
//! types in [`proto`]. Targets lower (see [`lower`]) to [`proto::BuildAction`]s
//! that form a global DAG ([`graph`]), executed wave-by-wave ([`executor`]) with
//! a content-addressed action cache ([`cache`]).
//!
//! The crate is standalone — it has no `tddy-*` dependencies.

pub mod proto {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/tddy.build.v1.rs"));
}

pub mod cache;
pub mod discovery;
pub mod error;
pub mod executor;
pub mod graph;
pub mod lower;
pub mod manifest;
pub mod serde_helpers;
pub mod service;

pub use error::BuildError;
pub use manifest::load_build_manifest;

pub use proto::{
    ActionCacheEntry, ActionType, BuildAction, BuildManifest, BuildTarget, DockerImageTarget,
    FileFingerprint, FileSet, OutputDecl, OutputKind, RustBinaryTarget, RustLibraryTarget,
    ScriptTarget, TargetGroupTarget, ToolTarget, TypeScriptTarget,
};
