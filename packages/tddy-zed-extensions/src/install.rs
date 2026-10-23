//! Install, list and remove (R1–R6).
//!
//! Both sources share one pipeline: stage into `installed/.<id>.partial`, validate the staged tree,
//! then decide against the index and swap. A refusal before the swap removes the partial directory
//! and leaves the previous install and the index untouched.

use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use reqwest::Url;
use serde::Deserialize;

use crate::archive;
use crate::dir::ExtensionsDir;
use crate::error::ExtensionError;
use crate::index::{ExtensionSource, Index, IndexEntry};
use crate::manifest::ExtensionManifest;
use crate::registry::Registry;
use crate::staging::{copy_tree, remove_tree, swap_in, Staging};

const MANIFEST: &str = "extension.toml";
const WASM: &str = "extension.wasm";
const LANGUAGE_CONFIG: &str = "config.toml";

/// How long one registry download may take, connect to last byte, before it is refused.
const REGISTRY_TIMEOUT: Duration = Duration::from_secs(120);

/// The R4 query: schema 0..=1 and Zed's `since_v0_6_0` WASM API range.
const REGISTRY_QUERY: [(&str, &str); 4] = [
    ("min_schema_version", "0"),
    ("max_schema_version", "1"),
    ("min_wasm_api_version", "0.6.0"),
    ("max_wasm_api_version", "0.7.0"),
];

/// What an install did (R5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Nothing of this id was installed before.
    Installed(IndexEntry),
    /// A different version was installed and is now replaced.
    Replaced {
        previous_version: String,
        entry: IndexEntry,
    },
    /// The same version is already installed; nothing changed.
    AlreadyInstalled(IndexEntry),
}

/// One installed extension and what it provides, for `list` (R6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledExtension {
    pub entry: IndexEntry,
    /// Language names, from each `languages/<x>/config.toml`'s `name`.
    pub languages: Vec<String>,
    /// Grammar names declared in `extension.toml`.
    pub grammars: Vec<String>,
    /// Language-server ids declared in `extension.toml`.
    pub language_servers: Vec<String>,
    /// Whether `extension.wasm` is present.
    pub has_wasm: bool,
}

impl InstalledExtension {
    /// `<id> <version> <source> — languages: …; grammars: …; language servers: …; wasm: yes|no`,
    /// an empty list printed as `none`.
    pub fn line(&self) -> String {
        let source = match self.entry.source {
            ExtensionSource::Registry => "registry",
            ExtensionSource::Path => "path",
        };
        let wasm = if self.has_wasm { "yes" } else { "no" };
        format!(
            "{} {} {} — languages: {}; grammars: {}; language servers: {}; wasm: {}",
            self.entry.id,
            self.entry.version,
            source,
            or_none(&self.languages),
            or_none(&self.grammars),
            or_none(&self.language_servers),
            wasm,
        )
    }
}

/// Downloads `id` (at `version`, else the latest compatible) from `registry` and installs it:
/// `GET {api}/extensions/{id}[/{version}]/download` with the R4 schema and WASM API range.
pub async fn install_from_registry(
    dir: &ExtensionsDir,
    registry: &Registry,
    id: &str,
    version: Option<&str>,
) -> Result<InstallOutcome, ExtensionError> {
    check_id(id)?;
    let url = download_url(registry, id, version)?;
    let body = fetch(id, &url).await?;
    let staging = Staging::begin(dir, id)?;
    archive::unpack(GzDecoder::new(&body[..]), staging.path(), id)?;
    let manifest = validate(staging.path(), id)?;
    commit(dir, staging, &manifest, ExtensionSource::Registry)
}

/// Copies the extension directory at `path` (a dev extension) in, under its manifest's id. Symlinks
/// in the source are followed, so the copy holds the content they name.
pub fn install_from_path(
    dir: &ExtensionsDir,
    path: &Path,
) -> Result<InstallOutcome, ExtensionError> {
    let source = read_source_manifest(path)?;
    check_id(&source.id)?;
    let staging = Staging::begin(dir, &source.id)?;
    copy_tree(path, staging.path())?;
    let manifest = validate(staging.path(), &source.id)?;
    commit(dir, staging, &manifest, ExtensionSource::Path)
}

/// Every installed extension, sorted by id.
pub fn list(dir: &ExtensionsDir) -> Result<Vec<InstalledExtension>, ExtensionError> {
    Index::load(dir)?
        .extensions
        .into_values()
        .map(|entry| describe(dir, entry))
        .collect()
}

/// Deletes `installed/<id>` and its index entry, returning the entry removed. An id with no index
/// entry counts as not installed, even when a directory of that name is left behind.
pub fn remove(dir: &ExtensionsDir, id: &str) -> Result<IndexEntry, ExtensionError> {
    let not_installed = || ExtensionError::NotInstalled {
        extension: id.to_string(),
    };
    if !is_plain_name(id) {
        return Err(not_installed());
    }
    let mut index = Index::load(dir)?;
    let entry = index.extensions.remove(id).ok_or_else(not_installed)?;
    index.save(dir)?;
    remove_tree(&dir.extension(id))?;
    Ok(entry)
}

fn or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

/// `{api}/extensions/{id}[/{version}]/download` with the R4 query.
fn download_url(
    registry: &Registry,
    id: &str,
    version: Option<&str>,
) -> Result<Url, ExtensionError> {
    let unreachable = |message: String| ExtensionError::RegistryUnreachable {
        extension: id.to_string(),
        url: registry.api().to_string(),
        message,
    };
    let mut url = Url::parse(registry.api()).map_err(|e| unreachable(e.to_string()))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| unreachable("the registry address cannot carry a path".to_string()))?;
        segments.push("extensions").push(id);
        if let Some(version) = version {
            segments.push(version);
        }
        segments.push("download");
    }
    url.query_pairs_mut().extend_pairs(REGISTRY_QUERY);
    Ok(url)
}

/// The gzip-compressed archive at `url`. A non-success status and a transport failure are refused.
async fn fetch(id: &str, url: &Url) -> Result<Vec<u8>, ExtensionError> {
    let unreachable = |message: String| ExtensionError::RegistryUnreachable {
        extension: id.to_string(),
        url: url.to_string(),
        message,
    };
    let client = reqwest::Client::builder()
        .timeout(REGISTRY_TIMEOUT)
        .build()
        .map_err(|e| unreachable(e.to_string()))?;
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|e| unreachable(e.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ExtensionError::RegistryStatus {
            extension: id.to_string(),
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    response
        .bytes()
        .await
        .map(|body| body.to_vec())
        .map_err(|e| unreachable(e.to_string()))
}

/// The manifest at the top of a dev extension directory, read before anything is copied: its id
/// names the install.
fn read_source_manifest(path: &Path) -> Result<ExtensionManifest, ExtensionError> {
    let extension = path.display().to_string();
    let file = path.join(MANIFEST);
    let text = fs::read_to_string(&file).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ExtensionError::ManifestMissing {
            extension: extension.clone(),
            file: PathBuf::from(MANIFEST),
        },
        _ => ExtensionError::io(&file, e),
    })?;
    ExtensionManifest::parse(&text).map_err(|e| unparsable(&extension, Path::new(MANIFEST), e))
}

/// Checks the staged tree in R3's order and returns its manifest. Each refusal names the extension
/// and the file, relative to the extension root.
fn validate(staged: &Path, requested: &str) -> Result<ExtensionManifest, ExtensionError> {
    let manifest_file = Path::new(MANIFEST);
    let text = fs::read_to_string(staged.join(MANIFEST)).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ExtensionError::ManifestMissing {
            extension: requested.to_string(),
            file: manifest_file.to_path_buf(),
        },
        _ => ExtensionError::io(&staged.join(MANIFEST), e),
    })?;
    let manifest =
        ExtensionManifest::parse(&text).map_err(|e| unparsable(requested, manifest_file, e))?;
    if manifest.id != requested {
        return Err(ExtensionError::IdMismatch {
            requested: requested.to_string(),
            found: manifest.id,
            file: manifest_file.to_path_buf(),
        });
    }
    manifest.check_schema_version(manifest_file)?;
    for grammar in manifest.grammars.keys() {
        let file = Path::new("grammars").join(format!("{grammar}.wasm"));
        if !archive::is_contained(&file) || !staged.join(&file).is_file() {
            return Err(ExtensionError::GrammarWasmMissing {
                extension: requested.to_string(),
                grammar: grammar.clone(),
                file,
            });
        }
    }
    for language in language_dir_names(staged)? {
        let file = Path::new("languages").join(&language).join(LANGUAGE_CONFIG);
        if !staged.join(&file).is_file() {
            return Err(ExtensionError::LanguageConfigMissing {
                extension: requested.to_string(),
                file,
            });
        }
        // Parsed here, so `list` never meets an installed config it cannot read.
        read_language_name(staged, &file, requested)?;
    }
    Ok(manifest)
}

/// The names of the directories under `root/languages/`, sorted; none when there is no
/// `languages/`.
fn language_dir_names(root: &Path) -> Result<Vec<String>, ExtensionError> {
    let languages = root.join("languages");
    let entries = match fs::read_dir(&languages) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(ExtensionError::io(&languages, e)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| ExtensionError::io(&languages, e))?;
        let file_type = entry
            .file_type()
            .map_err(|e| ExtensionError::io(&entry.path(), e))?;
        if file_type.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Decides the staged install against the index (R5), then swaps it in. `installed/` and the index
/// change only once the staged tree has been accepted.
fn commit(
    dir: &ExtensionsDir,
    staging: Staging,
    manifest: &ExtensionManifest,
    source: ExtensionSource,
) -> Result<InstallOutcome, ExtensionError> {
    let mut index = Index::load(dir)?;
    let previous = index.extensions.get(&manifest.id).cloned();
    if let Some(existing) = previous
        .as_ref()
        .filter(|existing| existing.version == manifest.version)
    {
        return Ok(InstallOutcome::AlreadyInstalled(existing.clone()));
    }
    let entry = IndexEntry {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        source,
        installed_at: now_secs(dir)?,
        // R1: off for a fresh entry. An extension the user already opted in stays opted in when a
        // new version replaces it — the opt-in is the user's trust in the extension, not in one
        // version of its code.
        wasm_enabled: previous
            .as_ref()
            .is_some_and(|previous| previous.wasm_enabled),
    };
    index.extensions.insert(manifest.id.clone(), entry.clone());
    swap_in(dir, staging, &manifest.id, || index.save(dir))?;
    Ok(match previous {
        Some(previous) => InstallOutcome::Replaced {
            previous_version: previous.version,
            entry,
        },
        None => InstallOutcome::Installed(entry),
    })
}

/// What an installed extension provides, read from its installed directory.
fn describe(dir: &ExtensionsDir, entry: IndexEntry) -> Result<InstalledExtension, ExtensionError> {
    let root = dir.extension(&entry.id);
    let manifest_path = root.join(MANIFEST);
    let text =
        fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::io(&manifest_path, e))?;
    let manifest = ExtensionManifest::parse(&text)
        .map_err(|e| unparsable(&entry.id, Path::new(MANIFEST), e))?;
    let mut languages = Vec::new();
    for language in language_dir_names(&root)? {
        let file = Path::new("languages").join(&language).join(LANGUAGE_CONFIG);
        languages.push(read_language_name(&root, &file, &entry.id)?);
    }
    Ok(InstalledExtension {
        languages,
        grammars: manifest.grammars.into_keys().collect(),
        language_servers: manifest.language_servers.into_keys().collect(),
        has_wasm: root.join(WASM).is_file(),
        entry,
    })
}

/// The `name` in `root/file`, a `languages/<x>/config.toml`.
fn read_language_name(root: &Path, file: &Path, extension: &str) -> Result<String, ExtensionError> {
    let path = root.join(file);
    let text = fs::read_to_string(&path).map_err(|e| ExtensionError::io(&path, e))?;
    let config: LanguageConfig =
        toml::from_str(&text).map_err(|e| unparsable(extension, file, e))?;
    Ok(config.name)
}

/// The one field of `languages/<x>/config.toml` that `list` shows.
#[derive(Deserialize)]
struct LanguageConfig {
    name: String,
}

fn now_secs(dir: &ExtensionsDir) -> Result<u64, ExtensionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .map_err(|e| ExtensionError::Io {
            path: dir.root().to_path_buf(),
            message: format!("the system clock is before the Unix epoch: {e}"),
        })
}

/// Refuses an id that is not one plain name, since it becomes a directory name under `installed/`.
fn check_id(id: &str) -> Result<(), ExtensionError> {
    if is_plain_name(id) {
        Ok(())
    } else {
        Err(ExtensionError::InvalidExtensionId { id: id.to_string() })
    }
}

fn is_plain_name(id: &str) -> bool {
    let mut parts = Path::new(id).components();
    matches!(parts.next(), Some(Component::Normal(name)) if name.to_str() == Some(id))
        && parts.next().is_none()
}

fn unparsable(extension: &str, file: &Path, error: impl Display) -> ExtensionError {
    ExtensionError::ManifestUnparsable {
        extension: extension.to_string(),
        file: file.to_path_buf(),
        message: error.to_string(),
    }
}
