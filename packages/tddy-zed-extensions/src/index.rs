//! `<extensions dir>/index.json`: one entry per installed extension (R1).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;

/// Where an installed extension came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionSource {
    Registry,
    Path,
}

/// One installed extension, as the index records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub version: String,
    pub source: ExtensionSource,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
}

/// The whole index, keyed by extension id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub extensions: BTreeMap<String, IndexEntry>,
}

impl Index {
    /// Reads `dir.index()`; a directory with no index yet is an empty index.
    pub fn load(dir: &ExtensionsDir) -> Result<Self, ExtensionError> {
        let _ = dir;
        // TODO(reshape-extension-install): implement
        todo!("Index::load")
    }

    /// Writes `dir.index()` atomically (temp file + rename).
    pub fn save(&self, dir: &ExtensionsDir) -> Result<(), ExtensionError> {
        let _ = dir;
        // TODO(reshape-extension-install): implement
        todo!("Index::save")
    }
}
