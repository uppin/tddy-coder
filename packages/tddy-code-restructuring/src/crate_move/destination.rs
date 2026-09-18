use crate::RestructureError;

use super::Result;

use std::path::Path;

/// Where a module is going, resolved from the plan's `to` and the destination's own manifest.
///
/// The crate *name* is never taken from the directory name: `packages/tddy-daemon-kernel` could
/// declare any `[package] name`, and a caller's `use` path needs the declared one with its hyphens
/// turned into underscores. Reading the manifest is the only correct source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// The destination crate's directory, relative to the repository root, as the plan gave it.
    pub dir: String,
    /// `[package] name` from the destination's `Cargo.toml` — e.g. `tddy-daemon-kernel`.
    pub package: String,
    /// The identifier a `use` path needs — `package` with `-` replaced by `_`.
    pub extern_name: String,
}

impl Destination {
    /// Read a destination from its `Cargo.toml`.
    ///
    /// Refuses a directory with no manifest rather than creating one: a plan that names a crate
    /// which does not exist is a plan defect, and scaffolding a crate is authoring, not moving.
    pub fn read(root: &Path, dir: &str) -> Result<Destination> {
        let manifest = root.join(dir).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|error| {
            RestructureError::MalformedPlan(format!(
                "`{dir}` is not a crate: {} could not be read ({error})",
                manifest.display()
            ))
        })?;
        let package = declared_package_name(&text).ok_or_else(|| {
            RestructureError::MalformedPlan(format!(
                "{} declares no `[package] name`",
                manifest.display()
            ))
        })?;

        Ok(Destination {
            dir: dir.to_string(),
            package: package.to_string(),
            extern_name: package.replace('-', "_"),
        })
    }
}

/// The `[package] name` a manifest declares, read without a TOML parser.
///
/// One key of one table is all this needs, and the shape it has to survive is a workspace manifest
/// where `[dependencies]` and `[[bin]]` also carry a `name`. Scoping the search to the lines between
/// `[package]` and the next table header is what keeps those out; a dependency's name being returned
/// as the crate's would produce a `use` path that compiles nowhere.
fn declared_package_name(manifest: &str) -> Option<&str> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "[package]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "name")
        .and_then(|(_, value)| quoted(value))
}

/// The contents of the first double-quoted string in `value`, or `None` if it is not one.
fn quoted(value: &str) -> Option<&str> {
    let opened = value.trim_start().strip_prefix('"')?;
    opened.find('"').map(|end| &opened[..end])
}
