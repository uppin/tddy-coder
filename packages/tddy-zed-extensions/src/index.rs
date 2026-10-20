//! `<extensions dir>/index.json`: one entry per installed extension (R1).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

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
        let path = dir.index();
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| ExtensionError::Io {
                path,
                message: format!("does not parse: {e}"),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ExtensionError::io(&path, e)),
        }
    }

    /// Writes `dir.index()` atomically: a temp file beside it, synced, then renamed over it.
    pub fn save(&self, dir: &ExtensionsDir) -> Result<(), ExtensionError> {
        let root = dir.root();
        fs::create_dir_all(root).map_err(|e| ExtensionError::io(root, e))?;
        let path = dir.index();
        let json = serde_json::to_string_pretty(self).map_err(|e| ExtensionError::Io {
            path: path.clone(),
            message: format!("cannot serialise: {e}"),
        })?;
        let temp = root.join(format!("index.json.{}.tmp", std::process::id()));
        let result = write_synced(&temp, &json)
            .and_then(|()| fs::rename(&temp, &path).map_err(|e| ExtensionError::io(&path, e)));
        if result.is_err() {
            // Best effort: the failed save is the error the caller sees.
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

fn write_synced(path: &Path, text: &str) -> Result<(), ExtensionError> {
    let mut file = fs::File::create(path).map_err(|e| ExtensionError::io(path, e))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| ExtensionError::io(path, e))
}
