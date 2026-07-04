//! Unit tests: a subagent's un-windowed READ must bound how much file content it feeds back into
//! the model's context — the root cause of the fastcontext runaway in session 019f2d14, where 129
//! whole-file READs ballooned a 32k-token window (3 KB → 100 KB) and produced 100s-long prefill
//! spikes. A file longer than the default cap must come back capped, flagged truncated, with the
//! file's true length reported so the model can page instead of re-reading blindly. The READ tool
//! schema must also advertise `offset`/`limit` so the model knows paging is possible.
//!
//! Root-cause: packages/tddy-discovery/src/subagent.rs (`CodebaseAccess::read`) and
//! packages/tddy-discovery/src/openai.rs (`discovery_tool_definitions`).

use tddy_discovery::openai::discovery_tool_definitions;
use tddy_discovery::subagent::CodebaseAccess;

/// Default number of lines a single un-windowed READ returns before truncating. A file longer than
/// this must come back capped, with `truncated: true`, so the context can't blow up in one call.
const DEFAULT_READ_LINE_CAP: usize = 200;

/// A deterministic file body of `count` lines: `line 0`, `line 1`, … `line {count-1}`, joined with
/// newlines and no trailing newline — so the capped prefix is an exact substring we can assert on.
fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write `body` to a fresh temp file and return the dir guard (kept alive) plus its path string.
fn a_file_containing(body: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("source.rs");
    std::fs::write(&path, body).expect("write temp file");
    let path_str = path.to_str().expect("utf-8 path").to_string();
    (dir, path_str)
}

// ─── Fluent assertion over a READ result ────────────────────────────────────────

struct ReadResult(serde_json::Value);

fn read_result(value: serde_json::Value) -> ReadResult {
    ReadResult(value)
}

impl ReadResult {
    fn has_content(&self, expected: &str) -> &Self {
        let actual = self.0["content"]
            .as_str()
            .expect("READ result must carry a string 'content' field");
        assert_eq!(actual, expected, "READ content mismatch");
        self
    }

    fn is_truncated(&self, expected: bool) -> &Self {
        let actual = self.0["truncated"].as_bool().unwrap_or_else(|| {
            panic!(
                "READ result must carry a boolean 'truncated' field so the model knows whether \
                 more of the file remains; got: {}",
                self.0
            )
        });
        assert_eq!(actual, expected, "READ truncation flag mismatch");
        self
    }

    fn has_total_lines(&self, expected: usize) -> &Self {
        let actual = self.0["total_lines"].as_u64().unwrap_or_else(|| {
            panic!(
                "READ result must report the file's true 'total_lines' so the model can page; \
                 got: {}",
                self.0
            )
        });
        assert_eq!(actual as usize, expected, "READ total_lines mismatch");
        self
    }
}

// ─── Default cap ─────────────────────────────────────────────────────────────

/// A file longer than the default cap comes back capped — not the whole file dumped into context.
#[tokio::test]
async fn read_caps_a_file_longer_than_the_default_line_limit() {
    // Given — a 500-line file, read with no explicit window
    let (_dir, path) = a_file_containing(&numbered_lines(500));

    // When
    let result = CodebaseAccess::Local
        .read(&path)
        .await
        .expect("READ of an existing file must succeed");

    // Then — only the first 200 lines come back, flagged truncated, with the true length reported
    read_result(result)
        .has_content(&numbered_lines(DEFAULT_READ_LINE_CAP))
        .is_truncated(true)
        .has_total_lines(500);
}

/// A file within the cap comes back verbatim and is explicitly marked not truncated.
#[tokio::test]
async fn read_returns_a_file_within_the_cap_verbatim_and_not_truncated() {
    // Given — a 2-line file, comfortably under the cap
    let body = "fn main() {}\nfn helper() {}";
    let (_dir, path) = a_file_containing(body);

    // When
    let result = CodebaseAccess::Local
        .read(&path)
        .await
        .expect("READ of a small file must succeed");

    // Then — byte-for-byte identical content, not truncated
    read_result(result)
        .has_content(body)
        .is_truncated(false)
        .has_total_lines(2);
}

// ─── Tool schema advertises paging ──────────────────────────────────────────────

/// The READ tool schema sent to the model advertises `offset` and `limit`, so a model that reads a
/// large file can page through it rather than being forced to swallow the whole thing.
#[test]
fn read_tool_schema_advertises_offset_and_limit_parameters() {
    // Given / When
    let read_def = discovery_tool_definitions()
        .into_iter()
        .find(|d| d.function.name == "READ")
        .expect("discovery tools must include a READ definition");

    // Then
    let properties = &read_def.function.parameters["properties"];
    assert!(
        properties.get("offset").is_some(),
        "READ schema must advertise an 'offset' parameter; got: {properties}"
    );
    assert!(
        properties.get("limit").is_some(),
        "READ schema must advertise a 'limit' parameter; got: {properties}"
    );
}
