//! tddy-tools library: schema validation and CLI utilities.
//!
//! The binary is the primary interface; the library exposes schema validation
//! for testing and programmatic use.
//!
//! - [`schema`] — embedded JSON Schemas, [`validate_output`], `get-schema` payload.
//! - [`schema_manifest`] — goal registry from `schema-manifest.json` (`list-schemas`).

pub mod schema;
pub mod schema_manifest;
pub mod session_context;
