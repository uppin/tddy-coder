//! Unit tests: a subagent's READ returns the window the model asked for — and, when it asked for
//! none, the whole file.
//!
//! This layer used to substitute a 200-line cap for an absent `limit`, added after session
//! 019f2d14 where 129 whole-file READs ballooned a 32k window. It was removed after session
//! 01a0e285 showed the other side of that trade: capping only helps a model that then *pages*,
//! and this one re-read the first 200 lines of a 960-line file **nine times**, each turn
//! re-sending a history that already held the answer, with latency climbing 6s → 168s. A cap the
//! model did not ask for is one it cannot reason about.
//!
//! What remains is the machinery that lets a model bound its own reads, which is the part worth
//! keeping: `offset`/`limit` honoured exactly as given, `truncated` and `total_lines` on every
//! result, and both advertised in the tool schema so paging is discoverable.
//!
//! Root-cause: packages/tddy-discovery/src/subagent.rs (`CodebaseAccess::read`) and
//! packages/tddy-discovery/src/openai.rs (`discovery_tool_definitions`).

use tddy_discovery::openai::discovery_tool_definitions;
use tddy_discovery::subagent::CodebaseAccess;

/// A window small enough that the fixtures below have lines on both sides of it.
const A_WINDOW: usize = 200;

/// A deterministic file body of `count` lines: `line 0`, `line 1`, … `line {count-1}`, joined with
/// newlines and no trailing newline — so a returned window is an exact substring we can assert on.
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

// ─── The model owns the window ───────────────────────────────────────────────

/// No window asked for, no window imposed — however long the file is.
#[tokio::test]
async fn an_unwindowed_read_returns_the_whole_file_however_long_it_is() {
    // Given — a 500-line file, read with no explicit window
    let (_dir, path) = a_file_containing(&numbered_lines(500));

    // When
    let result = CodebaseAccess::Local
        .read(&path)
        .await
        .expect("READ of an existing file must succeed");

    // Then — all of it comes back, and nothing claims to have been left out
    read_result(result)
        .has_content(&numbered_lines(500))
        .is_truncated(false)
        .has_total_lines(500);
}

/// The window the model *does* ask for is honoured exactly, and the result says what it missed —
/// which is what makes paging possible for a model that chooses to page.
#[tokio::test]
async fn a_read_windowed_by_the_model_returns_that_window_and_reports_the_rest() {
    // Given — a 500-line file, read with an explicit window
    let (_dir, path) = a_file_containing(&numbered_lines(500));

    // When
    let result = CodebaseAccess::Local
        .read_window(&path, Some(0), Some(A_WINDOW as u64))
        .await
        .expect("READ of an existing file must succeed");

    // Then — exactly that window, flagged truncated, with the file's true length
    read_result(result)
        .has_content(&numbered_lines(A_WINDOW))
        .is_truncated(true)
        .has_total_lines(500);
}

/// A short file comes back verbatim and is explicitly marked not truncated.
#[tokio::test]
async fn read_returns_a_file_within_the_cap_verbatim_and_not_truncated() {
    // Given — a 2-line file
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
