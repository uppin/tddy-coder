//! HTML coverage and duplicate-test reports.

use std::collections::BTreeMap;
use std::path::Path;

use crate::complexity::file_complexity;
use crate::coverage::{load_per_test_meta, load_rust_final, TestMeta};
use crate::crap::{join_rust_function_metrics, FunctionMetric, JoinResult};
use crate::duplicate_tests::{analyze_coverage_dir, DuplicateAnalysis};
use crate::error::Result;

/// Console summary + `coverage/report.html` with Highest CRAP leaderboard.
pub fn generate_report(coverage_dir: &Path, crate_root: &Path) -> Result<JoinResult> {
    let rust_final = load_rust_final(coverage_dir)?;
    let _metas = load_per_test_meta(coverage_dir)?;

    let mut complexity_by_file = BTreeMap::new();
    for entry in walkdir::WalkDir::new(crate_root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.to_string_lossy().contains("/target/") {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let measured = file_complexity(&text)?;
        complexity_by_file.insert(abs.display().to_string(), measured);
    }

    let joined = join_rust_function_metrics(&rust_final, &complexity_by_file);
    let html = render_coverage_html(&joined);
    std::fs::write(coverage_dir.join("report.html"), html)?;

    eprintln!(
        "CRAP join: {:.1}% ({} matched / {} instrumented, {} unmatched)",
        joined.join_rate * 100.0,
        joined.functions.len(),
        joined.functions.len() + joined.unmatched_functions,
        joined.unmatched_functions
    );

    Ok(joined)
}

fn render_coverage_html(joined: &JoinResult) -> String {
    let mut rows = String::new();
    for metric in joined.functions.iter().take(50) {
        rows.push_str(&format!(
            "<tr><td>{:.1}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            metric.crap,
            html_escape(&metric.file),
            metric.line,
            html_escape(&metric.name),
            metric.complexity,
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Rust Coverage Report</title>
<style>body{{font-family:system-ui;margin:2rem}}table{{border-collapse:collapse}}td,th{{border:1px solid #ccc;padding:.4rem .6rem}}</style>
</head><body>
<h1>Highest CRAP</h1>
<table><thead><tr><th>CRAP</th><th>File</th><th>Line</th><th>Function</th><th>Complexity</th></tr></thead>
<tbody>{rows}</tbody></table>
</body></html>"#
    )
}

/// Write duplicate-tests HTML pages under `out_dir`.
pub fn generate_duplicate_tests_report(
    coverage_dir: &Path,
    out_dir: &Path,
    min_signature: usize,
    subset_ratio: f64,
    include_test_sources: bool,
) -> Result<DuplicateAnalysis> {
    std::fs::create_dir_all(out_dir)?;
    let analysis = analyze_coverage_dir(
        coverage_dir,
        min_signature,
        subset_ratio,
        include_test_sources,
    )?;

    let identical_html = render_duplicate_html("Identical test signatures", &analysis.identical);
    std::fs::write(out_dir.join("duplicate-tests.html"), identical_html)?;

    let subset_html = render_subset_html(&analysis.subsets);
    std::fs::write(out_dir.join("subset-tests.html"), subset_html)?;

    Ok(analysis)
}

fn render_duplicate_html(title: &str, groups: &[crate::duplicate_tests::DuplicateGroup]) -> String {
    let mut body = String::new();
    for group in groups {
        body.push_str(&format!(
            "<h2>{} keys — {} tests</h2><ul>",
            group.signature_size,
            group.tests.len()
        ));
        for test in &group.tests {
            body.push_str(&format!("<li>{}</li>", html_escape(test)));
        }
        body.push_str("</ul>");
    }
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body><h1>{title}</h1>{body}</body></html>"
    )
}

fn render_subset_html(relations: &[crate::duplicate_tests::SubsetRelation]) -> String {
    let mut rows = String::new();
    for rel in relations {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.0}%</td></tr>",
            html_escape(&rel.subset),
            html_escape(&rel.superset),
            rel.subset_size,
            rel.superset_size,
            rel.ratio * 100.0,
        ));
    }
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Subset tests</title>
<style>table{{border-collapse:collapse}}td,th{{border:1px solid #ccc;padding:.4rem}}</style>
</head><body><h1>Subset relations</h1>
<table><thead><tr><th>Subset</th><th>Superset</th><th>Subset size</th><th>Superset size</th><th>Ratio</th></tr></thead>
<tbody>{rows}</tbody></table></body></html>"#
    )
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[allow(dead_code)]
fn _test_meta_line(_meta: &TestMeta) -> Option<u32> {
    _meta.line
}

#[allow(dead_code)]
fn _function_metric_crap(m: &FunctionMetric) -> f64 {
    m.crap
}
