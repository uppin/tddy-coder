//! Duplicate and subset test detection via interned signature bitsets.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::coverage::{load_per_test_meta, PerTestRustFile, RustRegion};
use crate::error::{AnalysisError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub signature_size: usize,
    pub tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubsetRelation {
    pub subset: String,
    pub superset: String,
    pub subset_size: usize,
    pub superset_size: usize,
    pub ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateAnalysis {
    pub identical: Vec<DuplicateGroup>,
    pub subsets: Vec<SubsetRelation>,
}

/// Signature key for a Rust region (`s:` or `b:` prefix).
pub fn rust_region_key(repo_rel: &str, region: &RustRegion) -> String {
    let prefix = if region.kind == "branch" { 'b' } else { 's' };
    format!(
        "{prefix}:{repo_rel}:{}:{}-{}:{}",
        region.start_line, region.start_col, region.end_line, region.end_col
    )
}

fn is_production_source(path: &str) -> bool {
    if path.contains("/test/") || path.contains("/tests/") {
        return false;
    }
    if path.contains("__tests__") || path.contains("__mocks__") {
        return false;
    }
    let lower = path.to_lowercase();
    !lower.ends_with(".test.rs") && !lower.ends_with(".spec.rs") && !lower.contains(".cy.")
}

/// Build signature keys for one per-test Rust artifact.
pub fn signature_for_rust_test(
    per_test: &BTreeMap<String, PerTestRustFile>,
    include_test_sources: bool,
) -> Vec<String> {
    let mut keys = Vec::new();
    for (file, data) in per_test {
        if !include_test_sources && !is_production_source(file) {
            continue;
        }
        for region in &data.regions {
            if region.count == 0 {
                continue;
            }
            if matches!(region.kind.as_str(), "skipped" | "gap" | "expansion") {
                continue;
            }
            keys.push(rust_region_key(file, region));
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

struct BitsetSignature {
    test_name: String,
    words: Vec<u32>,
}

fn build_bitsets(
    signatures: &[(String, Vec<String>)],
) -> (Vec<BitsetSignature>, BTreeMap<String, usize>) {
    let mut intern: BTreeMap<String, usize> = BTreeMap::new();
    for (_, keys) in signatures {
        for key in keys {
            let len = intern.len();
            intern.entry(key.clone()).or_insert(len);
        }
    }
    let word_count = intern.len().div_ceil(32);
    let mut bitsets = Vec::new();
    for (name, keys) in signatures {
        let mut words = vec![0u32; word_count];
        for key in keys {
            let Some(&bit) = intern.get(key) else {
                continue;
            };
            words[bit / 32] |= 1u32 << (bit % 32);
        }
        bitsets.push(BitsetSignature {
            test_name: name.clone(),
            words,
        });
    }
    (bitsets, intern)
}

fn contains(subset: &[u32], superset: &[u32]) -> bool {
    subset.iter().zip(superset).all(|(s, p)| (s & p) == *s)
}

/// Find identical signature groups and strict subset relations.
pub fn analyze_duplicates(
    signatures: &[(String, Vec<String>)],
    min_signature: usize,
    subset_ratio: f64,
) -> DuplicateAnalysis {
    let (bitsets, intern) = build_bitsets(signatures);
    let mut identical: Vec<DuplicateGroup> = Vec::new();
    let mut bucket: HashMap<String, Vec<String>> = HashMap::new();
    for entry in &bitsets {
        let key = entry
            .words
            .iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .join(",");
        bucket.entry(key).or_default().push(entry.test_name.clone());
    }
    for tests in bucket.values_mut() {
        if tests.len() >= min_signature {
            tests.sort();
            identical.push(DuplicateGroup {
                signature_size: bitsets
                    .iter()
                    .find(|b| b.test_name == tests[0])
                    .map(|b| b.words.iter().map(|w| w.count_ones()).sum::<u32>() as usize)
                    .unwrap_or(0),
                tests: tests.clone(),
            });
        }
    }

    let mut inverted: HashMap<usize, Vec<usize>> = HashMap::new();
    for (index, entry) in bitsets.iter().enumerate() {
        for (word_index, word) in entry.words.iter().enumerate() {
            for bit in 0..32 {
                if word & (1u32 << bit) != 0 {
                    let key = intern
                        .iter()
                        .find(|(_, pos)| **pos == word_index * 32 + bit)
                        .map(|(k, _)| k.clone());
                    if let Some(key) = key {
                        if let Some(pos) = intern.get(&key) {
                            inverted.entry(*pos).or_default().push(index);
                        }
                    }
                }
            }
        }
    }

    let mut subsets = Vec::new();
    for (subset_index, subset) in bitsets.iter().enumerate() {
        let subset_size = subset.words.iter().map(|w| w.count_ones()).sum::<u32>() as usize;
        if subset_size == 0 {
            continue;
        }
        let mut candidates = Vec::new();
        for (word_index, word) in subset.words.iter().enumerate() {
            for bit in 0..32 {
                if word & (1u32 << bit) != 0 {
                    if let Some(indices) = inverted.get(&(word_index * 32 + bit)) {
                        candidates.extend(indices.iter().copied());
                    }
                }
            }
        }
        candidates.retain(|&i| i != subset_index);
        candidates.sort_unstable();
        candidates.dedup();
        for superset_index in candidates {
            let superset = &bitsets[superset_index];
            let superset_size = superset.words.iter().map(|w| w.count_ones()).sum::<u32>() as usize;
            if superset_size <= subset_size {
                continue;
            }
            if !contains(&subset.words, &superset.words) {
                continue;
            }
            let ratio = subset_size as f64 / superset_size as f64;
            if ratio >= subset_ratio {
                subsets.push(SubsetRelation {
                    subset: subset.test_name.clone(),
                    superset: superset.test_name.clone(),
                    subset_size,
                    superset_size,
                    ratio,
                });
            }
        }
    }

    DuplicateAnalysis { identical, subsets }
}

/// Load per-test artifacts and run duplicate analysis.
pub fn analyze_coverage_dir(
    coverage_dir: &std::path::Path,
    min_signature: usize,
    subset_ratio: f64,
    include_test_sources: bool,
) -> Result<DuplicateAnalysis> {
    let metas = load_per_test_meta(coverage_dir)?;
    let mut signatures = Vec::new();
    for meta in metas {
        let path = coverage_dir
            .join("per-test")
            .join(format!("{}.rust.json", meta.id));
        if !path.is_file() {
            return Err(AnalysisError::MissingCoverage {
                path: path.display().to_string(),
            });
        }
        let per_test: BTreeMap<String, PerTestRustFile> =
            serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        let keys = signature_for_rust_test(&per_test, include_test_sources);
        signatures.push((meta.full_name, keys));
    }
    Ok(analyze_duplicates(&signatures, min_signature, subset_ratio))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::RustRegion;

    fn region(kind: &str, line: u32) -> RustRegion {
        RustRegion {
            start_line: line,
            start_col: 1,
            end_line: line,
            end_col: 10,
            count: 1,
            kind: kind.to_string(),
        }
    }

    #[test]
    fn identical_signatures_group_when_min_size_is_met() {
        // Given two tests covering the same production region
        let key = rust_region_key("/src/lib.rs", &region("code", 3));
        let signatures = vec![
            ("test_a".to_string(), vec![key.clone()]),
            ("test_b".to_string(), vec![key.clone()]),
            ("test_c".to_string(), vec![key.clone()]),
            ("test_d".to_string(), vec![key.clone()]),
            ("test_e".to_string(), vec![key.clone()]),
        ];

        // When
        let analysis = analyze_duplicates(&signatures, 5, 0.5);

        // Then
        assert_eq!(analysis.identical.len(), 1);
        assert_eq!(analysis.identical[0].tests.len(), 5);
    }

    #[test]
    fn subset_containment_uses_unsigned_word_comparison() {
        // Given a superset with bit 31 set and a subset with only lower bits
        let mut signatures = Vec::new();
        let mut keys_a = Vec::new();
        let mut keys_b = Vec::new();
        for i in 0..33 {
            let file = format!("/src/f{i}.rs");
            keys_b.push(rust_region_key(&file, &region("code", i + 1)));
            if i < 16 {
                keys_a.push(rust_region_key(&file, &region("code", i + 1)));
            }
        }
        signatures.push(("small".to_string(), keys_a));
        signatures.push(("large".to_string(), keys_b));

        // When
        let analysis = analyze_duplicates(&signatures, 100, 0.4);

        // Then
        assert!(
            analysis
                .subsets
                .iter()
                .any(|r| r.subset == "small" && r.superset == "large"),
            "expected strict subset relation, got {:?}",
            analysis.subsets
        );
    }
}
