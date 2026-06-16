//! tddy-build: Bazel-inspired, content-addressed build system — a generic engine
//! plus a wiring point for build plugins.
//!
//! `BUILD.yaml` manifests deserialize into the open serde schema in [`manifest`].
//! Each target's `config.type` selects a built-in structural type ([`builtin`]) or a
//! registered [`plugin::BuildPlugin`], which lowers it (see [`lower`]) to
//! [`proto::BuildAction`]s that form a global DAG ([`graph`]), executed wave-by-wave
//! ([`executor`]) with a content-addressed action cache ([`cache`]). `BuildAction`
//! and the cache types remain proto — the stable engine↔plugin contract.
//!
//! The crate is standalone — it has no `tddy-*` dependencies and no knowledge of any
//! specific ecosystem target type (those live in plugin crates).

pub mod proto {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/tddy.build.v1.rs"));
}

pub mod builtin;
pub mod cache;
pub mod discovery;
pub mod error;
pub mod executor;
pub mod graph;
pub mod io;
pub mod lower;
pub mod manifest;
pub mod plugin;
pub mod serde_helpers;
pub mod service;

pub use error::BuildError;
pub use io::{outputs_to_decls, srcs_to_inputs, OutputSpec};
pub use manifest::{load_build_manifest, BuildManifest, BuildTarget, TargetConfig};
pub use plugin::{BuildPlugin, LowerContext, PluginRegistry};

pub use proto::{
    ActionCacheEntry, ActionType, BuildAction, FileFingerprint, FileSet, OutputDecl, OutputKind,
};
