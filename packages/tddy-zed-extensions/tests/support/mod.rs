//! Test support for the install acceptance suite: an extensions dir in a temp data dir, archives
//! built from `tests/fixtures/demo-lang/` (PRD F2 a), and a fixture registry that answers only the
//! request R4 specifies.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;
use tddy_zed_extensions::{ExtensionManifest, ExtensionsDir, Registry};
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `tests/fixtures/demo-lang/` — a declarative extension: language `Demo`, grammar `demo`.
pub fn the_demo_lang_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demo-lang")
}

/// A temp tddy data dir and the extensions dir under it. Dropping it deletes both.
pub struct ADataDir {
    tmp: TempDir,
    pub extensions: ExtensionsDir,
}

pub fn a_data_dir() -> ADataDir {
    let tmp = TempDir::new().expect("temp data dir");
    let extensions = ExtensionsDir::under_data_dir(tmp.path());
    ADataDir { tmp, extensions }
}

impl ADataDir {
    pub fn path(&self) -> &Path {
        self.tmp.path()
    }

    /// The `version` the installed `extension.toml` of `id` declares, or `None` when `id` has no
    /// installed directory.
    pub fn installed_version_of(&self, id: &str) -> Option<String> {
        let text = fs::read_to_string(self.extensions.extension(id).join("extension.toml")).ok()?;
        Some(
            ExtensionManifest::parse(&text)
                .expect("installed manifest parses")
                .version,
        )
    }

    /// Every path under `installed/<id>/`, relative to it, sorted.
    pub fn installed_files_of(&self, id: &str) -> Vec<String> {
        let root = self.extensions.extension(id);
        let mut files = Vec::new();
        collect_files(&root, &root, &mut files);
        files.sort();
        files
    }

    /// The names under `installed/` — installed ids plus any leftover `.<id>.partial`.
    pub fn installed_dir_names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.extensions.installed())
            .map(|entries| {
                entries
                    .map(|e| {
                        e.expect("dir entry")
                            .file_name()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read installed dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else {
            out.push(
                path.strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

/// A `.tar.gz` extension archive, the shape Zed's registry serves: the extension's files at the
/// archive root.
pub struct AnArchive {
    files: BTreeMap<String, Vec<u8>>,
    raw_entries: Vec<(Vec<u8>, Vec<u8>)>,
    symlinks: Vec<(String, String)>,
}

/// The demo-lang fixture, as an archive.
pub fn the_demo_lang_archive() -> AnArchive {
    let root = the_demo_lang_fixture();
    let mut names = Vec::new();
    collect_files(&root, &root, &mut names);
    let files = names
        .into_iter()
        .map(|name| {
            let bytes = fs::read(root.join(&name)).expect("read fixture file");
            (name, bytes)
        })
        .collect();
    AnArchive {
        files,
        raw_entries: Vec::new(),
        symlinks: Vec::new(),
    }
}

impl AnArchive {
    /// Rewrites `extension.toml`'s top-level `version`.
    pub fn with_version(self, version: &str) -> Self {
        self.with_manifest_line("version", version)
    }

    /// Rewrites `extension.toml`'s `id`.
    pub fn with_id(self, id: &str) -> Self {
        self.with_manifest_line("id", id)
    }

    /// Drops one file (a path relative to the extension root).
    pub fn without(mut self, file: &str) -> Self {
        assert!(
            self.files.remove(file).is_some(),
            "the fixture has no {file} to drop"
        );
        self
    }

    /// Adds an entry whose name is written into the tar header verbatim — `tar`'s builder refuses
    /// `..` and absolute names, which is exactly what a hostile archive carries.
    pub fn with_raw_entry(mut self, name: &str, bytes: &[u8]) -> Self {
        self.raw_entries
            .push((name.as_bytes().to_vec(), bytes.to_vec()));
        self
    }

    /// Replaces (or adds) one file's content.
    pub fn with_file(mut self, file: &str, bytes: &[u8]) -> Self {
        self.files.insert(file.to_string(), bytes.to_vec());
        self
    }

    /// Adds a symlink entry at `name` pointing at `target`, written after every file, in call order.
    pub fn with_symlink(mut self, name: &str, target: &str) -> Self {
        self.symlinks.push((name.to_string(), target.to_string()));
        self
    }

    pub fn build(self) -> Vec<u8> {
        let mut tar = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (name, bytes) in &self.files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            tar.append_data(&mut header, name, bytes.as_slice())
                .expect("append fixture file");
        }
        for (name, bytes) in &self.raw_entries {
            let mut header = tar::Header::new_gnu();
            let gnu = header.as_gnu_mut().expect("gnu header");
            gnu.name[..name.len()].copy_from_slice(name);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            tar.append(&header, bytes.as_slice())
                .expect("append raw entry");
        }
        for (name, target) in &self.symlinks {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            tar.append_link(&mut header, name, target)
                .expect("append symlink");
        }
        tar.into_inner()
            .expect("finish tar")
            .finish()
            .expect("finish gzip")
    }

    fn with_manifest_line(mut self, key: &str, value: &str) -> Self {
        let manifest = self
            .files
            .get_mut("extension.toml")
            .expect("fixture has extension.toml");
        let text = String::from_utf8(manifest.clone()).expect("utf-8 manifest");
        let prefix = format!("{key} = ");
        let rewritten: Vec<String> = text
            .lines()
            .map(|line| {
                if line.starts_with(&prefix) {
                    format!("{key} = \"{value}\"")
                } else {
                    line.to_string()
                }
            })
            .collect();
        *manifest = (rewritten.join("\n") + "\n").into_bytes();
        self
    }
}

/// A local registry answering `GET /extensions/<id>[/<version>]/download` — and only with the R4
/// query parameters; any other request gets wiremock's 404.
pub struct AFixtureRegistry {
    server: MockServer,
}

pub async fn a_fixture_registry() -> AFixtureRegistry {
    AFixtureRegistry {
        server: MockServer::start().await,
    }
}

impl AFixtureRegistry {
    /// Serves `archive` as `id` at `version`.
    pub async fn serving(self, id: &str, version: &str, archive: Vec<u8>) -> Self {
        self.mount(format!("/extensions/{id}/{version}/download"), archive)
            .await;
        self
    }

    /// Serves `archive` as the latest compatible `id`.
    pub async fn serving_latest(self, id: &str, archive: Vec<u8>) -> Self {
        self.mount(format!("/extensions/{id}/download"), archive)
            .await;
        self
    }

    pub fn registry(&self) -> Registry {
        Registry::at(self.server.uri())
    }

    async fn mount(&self, at: String, archive: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path(at))
            .and(query_param("min_schema_version", "0"))
            .and(query_param("max_schema_version", "1"))
            .and(query_param("min_wasm_api_version", "0.6.0"))
            .and(query_param("max_wasm_api_version", "0.7.0"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/gzip")
                    .set_body_bytes(archive),
            )
            .mount(&self.server)
            .await;
    }
}
