//! llvm-cov coverage capture: instrumented `cargo test --no-run`, per-test profiles, merge + export.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::crap::{RustFileCoverage, RustFunctionRecord};
use crate::error::{AnalysisError, Result};

const REGION_KINDS: [&str; 5] = ["code", "expansion", "skipped", "gap", "branch"];
const FOREIGN_SOURCES_REGEX: &str = "(/cargo/registry/|/cargo/git/|/rustc/|/target/)";

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

    let instrumented = build_instrumented_tests(&manifest_dir)?;
    let tests = list_tests(&manifest_dir, &instrumented)?;

    let mut denominator: BTreeMap<String, DenominatorFile> = BTreeMap::new();

    for (index, test_name) in tests.iter().enumerate() {
        let profraw = std::env::temp_dir().join(format!("tddy-coverage-{index}.profraw"));
        let profdata = std::env::temp_dir().join(format!("tddy-coverage-{index}.profdata"));
        let profile_file = profraw.to_string_lossy();

        let status = run_single_test(&instrumented, test_name, &profile_file)?;
        let exported = export_profile(&profdata, &profraw, &manifest_dir)?;
        let normalized = normalize_export(&exported);

        let spec = manifest_dir.join("src").display().to_string();
        let id = test_artifact_id(&spec, test_name);
        let executed = split_executed(&normalized, &mut denominator);

        let meta = TestMeta {
            id: id.clone(),
            name: test_name.clone(),
            full_name: test_name.clone(),
            spec,
            line: None,
            status,
            duration_ms: 0,
            lang: "rust".to_string(),
        };

        std::fs::write(
            per_test.join(format!("{id}.meta.json")),
            serde_json::to_string_pretty(&meta)?,
        )?;
        std::fs::write(
            per_test.join(format!("{id}.rust.json")),
            serde_json::to_string_pretty(&executed)?,
        )?;

        let _ = std::fs::remove_file(&profraw);
        let _ = std::fs::remove_file(&profdata);
    }

    write_denominator(coverage_dir, &denominator)?;
    Ok(())
}

#[derive(Default)]
struct DenominatorFile {
    regions: BTreeMap<String, RustRegion>,
    functions: BTreeMap<String, RustFunctionRecord>,
}

fn build_instrumented_tests(manifest_dir: &Path) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .current_dir(manifest_dir)
        .env(
            "RUSTFLAGS",
            "-C instrument-coverage -C link-arg=-fuse-ld=lld",
        )
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if value.get("reason").and_then(|v| v.as_str()) == Some("compiler-artifact") {
                if let Some(path) = value
                    .pointer("/executable")
                    .and_then(|v| v.as_str())
                    .filter(|p| !p.is_empty())
                {
                    return Ok(PathBuf::from(path));
                }
            }
        }
    }

    // Fallback: locate most recent test binary in target/debug/deps
    let deps = manifest_dir.join("target/debug/deps");
    let newest = std::fs::read_dir(&deps)
        .map_err(AnalysisError::Io)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.contains('.') && std::fs::metadata(e.path()).is_ok())
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
    newest
        .map(|e| e.path())
        .ok_or_else(|| AnalysisError::Cargo("could not locate instrumented test binary".into()))
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
        .args([
            "export",
            "-instr-profile",
            &profdata.to_string_lossy(),
            "-format=text",
            &format!("-ignore-filename-regex={FOREIGN_SOURCES_REGEX}"),
        ])
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
