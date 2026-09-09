//! llvm-cov coverage capture: instrumented `cargo test --no-run`, per-test profiles, merge + export.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::crap::{RustFileCoverage, RustFunctionRecord};
use crate::error::{AnalysisError, Result};

const REGION_KINDS: [&str; 5] = ["code", "expansion", "skipped", "gap", "branch"];
/// Path fragments identifying sources that are not part of the crate under
/// analysis: vendored dependencies, the toolchain's own `library/`, and build
/// output. Held as plain substrings so one list can serve both as an llvm-cov
/// `-ignore-filename-regex` and as a Rust-side predicate; none contains a regex
/// metacharacter, so joining them with `|` is a faithful translation.
///
/// Note the cargo entries carry no leading `/`. A registry checkout lives under
/// `$CARGO_HOME`, which defaults to `~/.cargo` — so a pattern anchored at
/// `/cargo/registry/` never matches `/Users/dev/.cargo/registry/`, and every
/// dependency's functions land in the CRAP denominator unmeasured.
const FOREIGN_SOURCE_MARKERS: [&str; 4] = ["cargo/registry/", "cargo/git/", "/rustc/", "/target/"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RustRegion {
    #[serde(rename = "startLine")]
    pub start_line: u32,
    #[serde(rename = "startCol")]
    pub start_col: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    #[serde(rename = "endCol")]
    pub end_col: u32,
    pub count: u32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerTestRustFile {
    pub regions: Vec<RustRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestMeta {
    pub id: String,
    pub name: String,
    pub full_name: String,
    pub spec: String,
    pub line: Option<u32>,
    pub status: String,
    #[serde(rename = "durationMs")]
    pub duration_ms: u32,
    pub lang: String,
}

/// Stable region identity shared with per-test artifacts.
pub fn region_key(region: &RustRegion) -> String {
    format!(
        "{}:{}-{}:{}",
        region.start_line, region.start_col, region.end_line, region.end_col
    )
}

/// md5(spec + NUL + name), first 16 hex chars — matches qape artifact ids.
pub fn test_artifact_id(spec: &str, name: &str) -> String {
    let digest = md5::compute(format!("{spec}\0{name}"));
    format!("{digest:x}").chars().take(16).collect()
}

fn llvm_tool(name: &str) -> Result<PathBuf> {
    which::which(name).map_err(|_| AnalysisError::MissingLlvmTool {
        tool: name.to_string(),
    })
}

fn cargo_manifest_dir(crate_path: &Path) -> Result<PathBuf> {
    let manifest = if crate_path.is_dir() {
        crate_path.join("Cargo.toml")
    } else {
        crate_path.to_path_buf()
    };
    if !manifest.is_file() {
        return Err(AnalysisError::Message(format!(
            "no Cargo.toml at {}",
            manifest.display()
        )));
    }
    Ok(manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".")))
}

/// Capture per-test Rust coverage for the crate at `crate_path`, writing into `coverage_dir`.
pub fn capture_coverage(crate_path: &Path, coverage_dir: &Path) -> Result<()> {
    let manifest_dir = cargo_manifest_dir(crate_path)?;
    let per_test = coverage_dir.join("per-test");
    std::fs::create_dir_all(&per_test)?;

    let context = CaptureContext {
        manifest_dir: &manifest_dir,
        per_test: &per_test,
    };
    let mut denominator: BTreeMap<String, DenominatorFile> = BTreeMap::new();

    for harness in build_instrumented_tests(&manifest_dir)? {
        for test_name in list_tests(&manifest_dir, &harness.executable)? {
            capture_one_test(&context, &harness, &test_name, &mut denominator)?;
        }
    }

    write_denominator(coverage_dir, &denominator)?;
    Ok(())
}

/// Paths shared by every per-test capture, kept together so
/// [`capture_one_test`] stays within a readable parameter count.
struct CaptureContext<'a> {
    manifest_dir: &'a Path,
    per_test: &'a Path,
}

/// Run one test under its own profile, export it, and write its two artifacts.
fn capture_one_test(
    context: &CaptureContext<'_>,
    harness: &TestHarness,
    test_name: &str,
    denominator: &mut BTreeMap<String, DenominatorFile>,
) -> Result<()> {
    let id = test_artifact_id(&harness.spec, test_name);
    // Keyed by artifact id, not a running index: ids are unique across
    // harnesses, so concurrent suites cannot clobber each other's profiles.
    let profraw = std::env::temp_dir().join(format!("tddy-coverage-{id}.profraw"));
    let profdata = std::env::temp_dir().join(format!("tddy-coverage-{id}.profdata"));

    let status = run_single_test(&harness.executable, test_name, &profraw.to_string_lossy())?;
    let exported = export_profile(
        &profdata,
        &profraw,
        context.manifest_dir,
        &harness.executable,
    )?;
    let executed = split_executed(&normalize_export(&exported), denominator);

    let meta = TestMeta {
        id: id.clone(),
        name: test_name.to_string(),
        full_name: test_name.to_string(),
        spec: harness.spec.clone(),
        line: None,
        status,
        duration_ms: 0,
        lang: "rust".to_string(),
    };

    std::fs::write(
        context.per_test.join(format!("{id}.meta.json")),
        serde_json::to_string_pretty(&meta)?,
    )?;
    std::fs::write(
        context.per_test.join(format!("{id}.rust.json")),
        serde_json::to_string_pretty(&executed)?,
    )?;

    let _ = std::fs::remove_file(&profraw);
    let _ = std::fs::remove_file(&profdata);
    Ok(())
}

#[derive(Default)]
struct DenominatorFile {
    regions: BTreeMap<String, RustRegion>,
    functions: BTreeMap<String, RustFunctionRecord>,
}

/// Is an lld driver resolvable under any of the names clang accepts for
/// `-fuse-ld=lld`?
fn lld_on_path() -> bool {
    ["lld", "ld.lld", "ld64.lld"]
        .iter()
        .any(|driver| which::which(driver).is_ok())
}

/// `-C instrument-coverage`, plus `-fuse-ld=lld` only when lld is actually
/// present. lld is a link-time speedup, not a requirement: hardcoding it made
/// the instrumented build fail with `clang: error: invalid linker name in
/// argument '-fuse-ld=lld'` on every host that ships without it — the nix dev
/// shell on macOS, for one, which left `analyze coverage` unusable there.
fn instrumented_rustflags_for(lld_available: bool) -> String {
    let mut flags = String::from("-C instrument-coverage");
    if lld_available {
        flags.push_str(" -C link-arg=-fuse-ld=lld");
    }
    flags
}

fn instrumented_rustflags() -> String {
    instrumented_rustflags_for(lld_on_path())
}

/// The `-ignore-filename-regex` value handed to `llvm-cov export`, so foreign
/// sources are dropped before they reach us rather than after.
fn foreign_sources_regex() -> String {
    format!("({})", FOREIGN_SOURCE_MARKERS.join("|"))
}

/// Authoritative filter for [`FOREIGN_SOURCE_MARKERS`]. `llvm-cov`'s regex is a
/// pre-filter for payload size; this is what decides what gets analyzed.
fn is_foreign_source(path: &str) -> bool {
    FOREIGN_SOURCE_MARKERS
        .iter()
        .any(|marker| path.contains(marker))
}

/// `llvm-cov export` reads counters out of the instrumented binary itself, so
/// the object file is a required positional argument — omitting it fails with
/// `No filenames specified!`, which this crate then reports as "cargo failed".
fn export_args(binary: &Path, profdata: &Path) -> Vec<String> {
    vec![
        "export".to_string(),
        binary.display().to_string(),
        "-instr-profile".to_string(),
        profdata.display().to_string(),
        "-format=text".to_string(),
        format!("-ignore-filename-regex={}", foreign_sources_regex()),
    ]
}

/// One instrumented libtest harness built by `cargo test --no-run`.
#[derive(Debug, Clone, PartialEq)]
struct TestHarness {
    /// The binary to enumerate with `--list` and run per test.
    executable: PathBuf,
    /// The test target's source file, used as each artifact's `spec` so tests
    /// sharing a name across suites keep distinct ids.
    spec: String,
}

/// Select the runnable libtest harnesses from `cargo test --no-run` JSON.
///
/// `cargo test --no-run` also emits the crate's own `[[bin]]` targets with an
/// `executable` set. Those know nothing of libtest, so `--list` on one prints
/// usage and exits non-zero; only a target built in test mode can be captured.
fn test_harnesses_from_cargo_json(stdout: &str) -> Vec<TestHarness> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.get("reason").and_then(|v| v.as_str()) == Some("compiler-artifact"))
        .filter(|value| value.pointer("/profile/test").and_then(|v| v.as_bool()) == Some(true))
        .filter_map(|value| {
            let executable = value
                .pointer("/executable")
                .and_then(|v| v.as_str())
                .filter(|path| !path.is_empty())?;
            let spec = value
                .pointer("/target/src_path")
                .and_then(|v| v.as_str())
                .unwrap_or(executable);
            Some(TestHarness {
                executable: PathBuf::from(executable),
                spec: spec.to_string(),
            })
        })
        .collect()
}

fn build_instrumented_tests(manifest_dir: &Path) -> Result<Vec<TestHarness>> {
    let output = Command::new("cargo")
        .current_dir(manifest_dir)
        .env("RUSTFLAGS", instrumented_rustflags())
        .args([
            "test",
            "--no-run",
            "--message-format=json-render-diagnostics",
        ])
        .output()
        .map_err(|e| AnalysisError::Cargo(e.to_string()))?;

    if !output.status.success() {
        return Err(AnalysisError::Cargo(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let harnesses = test_harnesses_from_cargo_json(&String::from_utf8_lossy(&output.stdout));
    if harnesses.is_empty() {
        return Err(AnalysisError::Cargo(
            "`cargo test --no-run` produced no libtest harness to capture".into(),
        ));
    }
    Ok(harnesses)
}

fn list_tests(manifest_dir: &Path, binary: &Path) -> Result<Vec<String>> {
    let output = Command::new(binary)
        .current_dir(manifest_dir)
        .arg("--list")
        .output()
        .map_err(|e| AnalysisError::Cargo(e.to_string()))?;
    if !output.status.success() {
        return Err(AnalysisError::Cargo(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_suffix(": test").map(str::trim))
        .map(str::to_string)
        .collect())
}

fn run_single_test(binary: &Path, test_name: &str, profile_file: &str) -> Result<String> {
    let output = Command::new(binary)
        .env("LLVM_PROFILE_FILE", profile_file)
        .args(["--exact", test_name, "--nocapture"])
        .output()
        .map_err(|e| AnalysisError::Cargo(e.to_string()))?;
    Ok(if output.status.success() {
        "passed".to_string()
    } else {
        "failed".to_string()
    })
}

fn export_profile(
    profdata: &Path,
    profraw: &Path,
    manifest_dir: &Path,
    binary: &Path,
) -> Result<serde_json::Value> {
    let llvm_profdata = llvm_tool("llvm-profdata")?;
    let llvm_cov = llvm_tool("llvm-cov")?;

    let merge = Command::new(&llvm_profdata)
        .args(["merge", "-sparse", &profraw.to_string_lossy(), "-o"])
        .arg(profdata)
        .output()
        .map_err(|e| AnalysisError::Cargo(e.to_string()))?;
    if !merge.status.success() {
        return Err(AnalysisError::Cargo(
            String::from_utf8_lossy(&merge.stderr).into_owned(),
        ));
    }

    let export = Command::new(&llvm_cov)
        .current_dir(manifest_dir)
        .args(export_args(binary, profdata))
        .output()
        .map_err(|e| AnalysisError::Cargo(e.to_string()))?;
    if !export.status.success() {
        return Err(AnalysisError::Cargo(
            String::from_utf8_lossy(&export.stderr).into_owned(),
        ));
    }

    serde_json::from_slice(&export.stdout).map_err(AnalysisError::from)
}

fn normalize_export(exported: &serde_json::Value) -> BTreeMap<String, RustFileCoverageWithRegions> {
    let mut by_file: BTreeMap<String, RustFileCoverageWithRegions> = BTreeMap::new();

    for dataset in exported
        .get("data")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        for function in dataset
            .get("functions")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let mut func_line = u32::MAX;
            let filenames: Vec<String> = function
                .get("filenames")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            for region in function
                .get("regions")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                let Some(region_arr) = region.as_array() else {
                    continue;
                };
                if region_arr.len() < 8 {
                    continue;
                }
                let file_id = region_arr[5].as_u64().unwrap_or(0) as usize;
                let Some(region_file) = filenames.get(file_id) else {
                    continue;
                };
                if is_foreign_source(region_file) {
                    continue;
                }
                let kind_idx = region_arr[7].as_u64().unwrap_or(0) as usize;
                let kind = REGION_KINDS
                    .get(kind_idx)
                    .copied()
                    .unwrap_or("code")
                    .to_string();
                let rust_region = RustRegion {
                    start_line: region_arr[0].as_u64().unwrap_or(0) as u32,
                    start_col: region_arr[1].as_u64().unwrap_or(0) as u32,
                    end_line: region_arr[2].as_u64().unwrap_or(0) as u32,
                    end_col: region_arr[3].as_u64().unwrap_or(0) as u32,
                    count: region_arr[4].as_u64().unwrap_or(0) as u32,
                    kind: kind.clone(),
                };
                let start_line = rust_region.start_line;
                let entry = by_file.entry(region_file.clone()).or_default();
                entry.regions.push(rust_region);
                if kind == "code" || kind == "branch" {
                    func_line = func_line.min(start_line);
                }
            }

            if func_line == u32::MAX {
                continue;
            }
            let Some(primary) = filenames.first() else {
                continue;
            };
            if is_foreign_source(primary) {
                continue;
            }
            let entry = by_file.entry(primary.clone()).or_default();
            let name = function
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let count = function.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let key = func_line.to_string();
            let record = RustFunctionRecord {
                name,
                line: func_line,
                count,
            };
            if let Some(existing) = entry.functions.get_mut(&key) {
                existing.count = existing.count.max(count);
            } else {
                entry.functions.insert(key, record);
            }
        }
    }

    by_file
}

#[derive(Default)]
struct RustFileCoverageWithRegions {
    regions: Vec<RustRegion>,
    functions: BTreeMap<String, RustFunctionRecord>,
}

fn split_executed(
    regions: &BTreeMap<String, RustFileCoverageWithRegions>,
    denominator: &mut BTreeMap<String, DenominatorFile>,
) -> BTreeMap<String, PerTestRustFile> {
    let mut executed = BTreeMap::new();
    for (file, file_data) in regions {
        let known = denominator.entry(file.clone()).or_default();
        for region in &file_data.regions {
            known.regions.insert(region_key(region), region.clone());
        }
        for record in file_data.functions.values() {
            let key = record.line.to_string();
            let prev = known.functions.get(&key).map(|r| r.count).unwrap_or(0);
            known.functions.insert(
                key,
                RustFunctionRecord {
                    name: record.name.clone(),
                    line: record.line,
                    count: prev.max(record.count),
                },
            );
        }
        let hits: Vec<RustRegion> = file_data
            .regions
            .iter()
            .filter(|r| r.count > 0)
            .cloned()
            .collect();
        if !hits.is_empty() {
            executed.insert(file.clone(), PerTestRustFile { regions: hits });
        }
    }
    executed
}

fn write_denominator(
    coverage_dir: &Path,
    denominator: &BTreeMap<String, DenominatorFile>,
) -> Result<()> {
    let final_map: BTreeMap<String, RustFileCoverage> = denominator
        .iter()
        .map(|(file, data)| {
            (
                file.clone(),
                RustFileCoverage {
                    regions: data.regions.values().cloned().collect(),
                    functions: data.functions.values().cloned().collect(),
                },
            )
        })
        .collect();
    let path = coverage_dir.join("rust-coverage-final.json");
    std::fs::write(path, serde_json::to_string_pretty(&final_map)?)?;
    Ok(())
}

/// Load `rust-coverage-final.json` or fail if missing.
pub fn load_rust_final(coverage_dir: &Path) -> Result<BTreeMap<String, RustFileCoverage>> {
    let path = coverage_dir.join("rust-coverage-final.json");
    if !path.is_file() {
        return Err(AnalysisError::MissingCoverage {
            path: path.display().to_string(),
        });
    }
    let contents = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Load per-test meta files from `coverage_dir/per-test/*.meta.json`.
pub fn load_per_test_meta(coverage_dir: &Path) -> Result<Vec<TestMeta>> {
    let per_test = coverage_dir.join("per-test");
    if !per_test.is_dir() {
        return Err(AnalysisError::MissingCoverage {
            path: per_test.display().to_string(),
        });
    }
    let mut metas = Vec::new();
    for entry in std::fs::read_dir(&per_test).map_err(AnalysisError::Io)? {
        let entry = entry.map_err(AnalysisError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".meta.json") {
            continue;
        }
        let contents = std::fs::read_to_string(&path)?;
        metas.push(serde_json::from_str(&contents)?);
    }
    if metas.is_empty() {
        return Err(AnalysisError::MissingCoverage {
            path: per_test.display().to_string(),
        });
    }
    Ok(metas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrumented_rustflags_always_request_coverage_instrumentation() {
        for lld_available in [true, false] {
            assert!(
                instrumented_rustflags_for(lld_available).contains("-C instrument-coverage"),
                "coverage instrumentation must not depend on the linker"
            );
        }
    }

    #[test]
    fn instrumented_rustflags_omit_lld_when_it_is_not_installed() {
        // Hosts without lld (the nix dev shell on macOS) previously failed the
        // instrumented build outright with `invalid linker name`.
        assert_eq!(
            instrumented_rustflags_for(false),
            "-C instrument-coverage",
            "must not name a linker the host does not have"
        );
    }

    #[test]
    fn instrumented_rustflags_use_lld_when_it_is_installed() {
        assert_eq!(
            instrumented_rustflags_for(true),
            "-C instrument-coverage -C link-arg=-fuse-ld=lld"
        );
    }

    // ---- gap 4: `llvm-cov export` needs the instrumented binary ----

    #[test]
    fn export_names_the_instrumented_binary_as_its_object_file() {
        let args = export_args(Path::new("/t/deps/daemon-abc"), Path::new("/t/x.profdata"));
        let export_at = args
            .iter()
            .position(|a| a == "export")
            .expect("export verb");
        assert_eq!(
            args.get(export_at + 1).map(String::as_str),
            Some("/t/deps/daemon-abc"),
            "llvm-cov export takes the object file positionally; without it \
             it fails with `No filenames specified!`"
        );
    }

    #[test]
    fn export_passes_the_merged_profile() {
        let args = export_args(Path::new("/t/bin"), Path::new("/t/x.profdata"));
        let flag = args
            .iter()
            .position(|a| a == "-instr-profile")
            .expect("-instr-profile");
        assert_eq!(
            args.get(flag + 1).map(String::as_str),
            Some("/t/x.profdata")
        );
    }

    // ---- gap 5: the foreign-source filter must actually match cargo paths ----

    #[test]
    fn vendored_dependency_sources_are_foreign() {
        // The default CARGO_HOME is `~/.cargo`, so the registry path contains
        // `/.cargo/registry/` — a pattern anchored at `/cargo/` never fires.
        for path in [
            "/Users/dev/.cargo/registry/src/index.crates.io-1/anyhow-1.0.102/src/chain.rs",
            "/home/dev/.cargo/git/checkouts/livekit-abc/src/room.rs",
            "/usr/local/cargo/registry/src/index/serde-1.0.0/src/lib.rs",
            "/rustc/9b00956e56009bab2aa15d7bff10916599e3d6d6/library/core/src/option.rs",
            "/work/repo/target/debug/build/tddy-daemon-123/out/gen.rs",
        ] {
            assert!(is_foreign_source(path), "should be filtered out: {path}");
        }
    }

    #[test]
    fn crate_sources_are_not_foreign() {
        for path in [
            "/work/repo/packages/tddy-daemon/src/connection_service.rs",
            "/work/repo/packages/tddy-daemon/tests/acceptance_daemon.rs",
            "/work/cargo-cult/src/lib.rs",
        ] {
            assert!(!is_foreign_source(path), "should be analyzed: {path}");
        }
    }

    #[test]
    fn the_ignore_regex_is_built_from_the_same_markers_as_the_predicate() {
        let regex = foreign_sources_regex();
        for marker in FOREIGN_SOURCE_MARKERS {
            assert!(regex.contains(marker), "regex must carry marker {marker}");
        }
    }

    // ---- gap 6: only libtest harnesses can be enumerated and run per-test ----

    fn artifact(name: &str, kind: &str, is_test: bool, executable: &str, src: &str) -> String {
        format!(
            r#"{{"reason":"compiler-artifact","target":{{"name":"{name}","kind":["{kind}"],"src_path":"{src}"}},"profile":{{"test":{is_test}}},"executable":"{executable}"}}"#
        )
    }

    #[test]
    fn the_crates_own_binary_is_not_mistaken_for_a_test_harness() {
        // `cargo test --no-run` emits the `[[bin]]` target first for tddy-daemon.
        // Running `tddy-daemon --list` just prints clap usage and exits non-zero.
        let stdout = [
            artifact(
                "tddy-daemon",
                "bin",
                false,
                "/t/debug/tddy-daemon",
                "src/main.rs",
            ),
            artifact(
                "acceptance",
                "test",
                true,
                "/t/deps/acceptance-1",
                "tests/acceptance.rs",
            ),
        ]
        .join("\n");

        let harnesses = test_harnesses_from_cargo_json(&stdout);

        assert_eq!(harnesses.len(), 1, "only the libtest harness is runnable");
        assert_eq!(
            harnesses[0].executable,
            PathBuf::from("/t/deps/acceptance-1")
        );
    }

    #[test]
    fn every_test_harness_is_captured_not_just_the_first() {
        // tddy-daemon builds 167 harnesses; capturing one reports the other 166
        // suites' code as entirely uncovered.
        let stdout = [
            artifact("unittests", "lib", true, "/t/deps/lib-1", "src/lib.rs"),
            artifact("a", "test", true, "/t/deps/a-2", "tests/a.rs"),
            artifact("b", "test", true, "/t/deps/b-3", "tests/b.rs"),
        ]
        .join("\n");

        let harnesses = test_harnesses_from_cargo_json(&stdout);

        assert_eq!(
            harnesses
                .iter()
                .map(|h| h.executable.clone())
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("/t/deps/lib-1"),
                PathBuf::from("/t/deps/a-2"),
                PathBuf::from("/t/deps/b-3")
            ]
        );
    }

    #[test]
    fn a_harness_is_specified_by_its_test_source_so_ids_stay_distinct() {
        // Two suites may both define `connects`; keying the artifact id on the
        // crate's src dir would collide them onto one file.
        let stdout = [
            artifact("a", "test", true, "/t/deps/a-2", "/repo/tests/a.rs"),
            artifact("b", "test", true, "/t/deps/b-3", "/repo/tests/b.rs"),
        ]
        .join("\n");

        let harnesses = test_harnesses_from_cargo_json(&stdout);

        assert_eq!(harnesses[0].spec, "/repo/tests/a.rs");
        assert_eq!(harnesses[1].spec, "/repo/tests/b.rs");
        assert_ne!(
            test_artifact_id(&harnesses[0].spec, "connects"),
            test_artifact_id(&harnesses[1].spec, "connects"),
            "same-named tests in different suites must not share an id"
        );
    }

    #[test]
    fn artifacts_without_an_executable_are_skipped() {
        let stdout = [
            r#"{"reason":"compiler-artifact","target":{"name":"serde","kind":["lib"],"src_path":"s.rs"},"profile":{"test":false},"executable":null}"#.to_string(),
            r#"{"reason":"build-script-executed","package_id":"x"}"#.to_string(),
            artifact("a", "test", true, "/t/deps/a-2", "tests/a.rs"),
        ]
        .join("\n");

        assert_eq!(test_harnesses_from_cargo_json(&stdout).len(), 1);
    }
}
