//! The file a tool session's route travels in, from the daemon to `tddy-coder`.
//!
//! A tool session is spawned with a closed environment allowlist — commit identity only, "never a
//! credential" — so its route cannot travel as environment. It travels the way an agent definition's
//! credential already does: a JSON file in the session directory, owner-only, named on the command
//! line (`--provider-route-path`). `tddy-coder` refuses a route file anyone else can read.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::provider_route::{HarnessRoute, RouteFile, RouteSecret};

/// The route file's name inside a session directory.
pub const PROVIDER_ROUTE_FILENAME: &str = "provider-route.json";

/// The file's shape. Every field is required and nothing else is accepted: a route that lost a
/// field on the way would run the harness with part of its configuration silently missing.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteJson {
    env: Vec<NameValue>,
    secret_env: Vec<NameValue>,
    files: Vec<FileJson>,
    args: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NameValue {
    name: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileJson {
    name: String,
    contents: String,
}

/// `route` as the route file holds it — secrets included, which is why the file is owner-only.
#[must_use]
pub fn route_to_json(route: &HarnessRoute) -> String {
    let json = RouteJson {
        env: route
            .env
            .iter()
            .map(|(name, value)| NameValue {
                name: name.clone(),
                value: value.clone(),
            })
            .collect(),
        secret_env: route
            .secret_env
            .iter()
            .map(|(name, secret)| NameValue {
                name: name.clone(),
                value: secret.expose().to_string(),
            })
            .collect(),
        files: route
            .files
            .iter()
            .map(|file| FileJson {
                name: file.name.clone(),
                contents: file.contents.clone(),
            })
            .collect(),
        args: route.args.clone(),
    };
    serde_json::to_string(&json).expect("a route of strings always serializes")
}

/// The route a route file holds.
///
/// A parse failure names the position and never the text, because the text may be a key and this
/// message reaches the logs.
pub fn route_from_json(json: &str) -> Result<HarnessRoute, String> {
    let parsed: RouteJson = serde_json::from_str(json).map_err(|e| {
        format!(
            "the provider route file is not a provider route (line {}, column {})",
            e.line(),
            e.column()
        )
    })?;
    Ok(HarnessRoute {
        env: parsed
            .env
            .into_iter()
            .map(|pair| (pair.name, pair.value))
            .collect(),
        secret_env: parsed
            .secret_env
            .into_iter()
            .map(|pair| (pair.name, RouteSecret::new(pair.value)))
            .collect(),
        files: parsed
            .files
            .into_iter()
            .map(|file| RouteFile {
                name: file.name,
                contents: file.contents,
            })
            .collect(),
        args: parsed.args,
    })
}

/// Read the route file at `path`, refusing one whose mode lets anyone but its owner read it.
///
/// The mode is read from the open file, so the one checked is the one read. Any group or other
/// permission bit is refused, not only group or other read: a group-writable route file lets
/// another account replace the endpoint and key a session runs on.
pub fn load_route_file(path: &Path) -> Result<HarnessRoute, String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        format!(
            "the provider route file {} could not be opened: {e}",
            path.display()
        )
    })?;
    let mode = file
        .metadata()
        .map_err(|e| {
            format!(
                "the provider route file {} could not be inspected: {e}",
                path.display()
            )
        })?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "the provider route file {} is readable by an account other than its owner (mode {mode:o}); \
             it holds a provider key, so only its owner may read it (mode 0600)",
            path.display()
        ));
    }
    let mut json = String::new();
    file.read_to_string(&mut json).map_err(|e| {
        format!(
            "the provider route file {} could not be read: {e}",
            path.display()
        )
    })?;
    route_from_json(&json)
}
