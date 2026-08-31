//! CRAP scoring and coverage↔complexity join on `(file, declaration line)`.

use serde::{Deserialize, Serialize};

use crate::complexity::FunctionComplexity;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionMetric {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub complexity: u32,
    pub covered: bool,
    pub crap: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JoinResult {
    pub functions: Vec<FunctionMetric>,
    pub join_rate: f64,
    pub unmatched_functions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustFunctionRecord {
    pub name: String,
    pub line: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RustFileCoverage {
    #[serde(default)]
    pub regions: Vec<crate::coverage::RustRegion>,
    #[serde(default)]
    pub functions: Vec<RustFunctionRecord>,
}

/// `CRAP = complexity² × (1 − coverage)³ + complexity`
pub fn crap_score(complexity: u32, coverage_ratio: f64) -> f64 {
    let uncovered = 1.0 - coverage_ratio;
    (complexity as f64).powi(2) * uncovered.powi(3) + complexity as f64
}

/// Join Rust coverage function records to syn-measured complexity.
pub fn join_rust_function_metrics(
    rust_final: &std::collections::BTreeMap<String, RustFileCoverage>,
    complexity_by_file: &std::collections::BTreeMap<String, Vec<FunctionComplexity>>,
) -> JoinResult {
    let mut functions = Vec::new();
    let mut instrumented_count = 0usize;

    for (file, file_coverage) in rust_final {
        let by_line: std::collections::HashMap<u32, &FunctionComplexity> = complexity_by_file
            .get(file)
            .map(|measured| measured.iter().map(|m| (m.line, m)).collect())
            .unwrap_or_default();

        for record in &file_coverage.functions {
            instrumented_count += 1;
            let Some(measured) = by_line.get(&record.line) else {
                continue;
            };
            let covered = record.count > 0;
            functions.push(FunctionMetric {
                file: file.clone(),
                name: measured.name.clone(),
                line: measured.line,
                complexity: measured.complexity,
                covered,
                crap: crap_score(measured.complexity, if covered { 1.0 } else { 0.0 }),
            });
        }
    }

    functions.sort_by(|a, b| {
        b.crap
            .partial_cmp(&a.crap)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });

    let matched = functions.len();
    JoinResult {
        functions,
        join_rate: if instrumented_count == 0 {
            1.0
        } else {
            matched as f64 / instrumented_count as f64
        },
        unmatched_functions: instrumented_count.saturating_sub(matched),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::FunctionComplexity;

    #[test]
    fn covered_function_scores_exactly_its_complexity() {
        // Given
        let complexity = 6;

        // When
        let score = crap_score(complexity, 1.0);

        // Then
        assert_eq!(score, 6.0);
    }

    #[test]
    fn untested_complex_function_scores_above_its_complexity() {
        // Given
        let complexity = 6;

        // When
        let score = crap_score(complexity, 0.0);

        // Then
        assert!(score > complexity as f64);
        assert_eq!(score, 42.0);
    }

    #[test]
    fn joins_on_declaration_line_not_function_name() {
        // Given
        let mut rust_final = std::collections::BTreeMap::new();
        rust_final.insert(
            "/src/lib.rs".to_string(),
            RustFileCoverage {
                regions: Vec::new(),
                functions: vec![RustFunctionRecord {
                    name: "inner::{closure#0}".to_string(),
                    line: 10,
                    count: 0,
                }],
            },
        );
        let mut complexity_by_file = std::collections::BTreeMap::new();
        complexity_by_file.insert(
            "/src/lib.rs".to_string(),
            vec![FunctionComplexity {
                name: "helper".to_string(),
                line: 10,
                complexity: 2,
            }],
        );

        // When
        let joined = join_rust_function_metrics(&rust_final, &complexity_by_file);

        // Then
        assert_eq!(joined.functions.len(), 1);
        assert_eq!(joined.functions[0].name, "helper");
        assert_eq!(joined.functions[0].crap, crap_score(2, 0.0));
    }
}
