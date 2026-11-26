//! Test support for crates migrating onto the port.

use std::path::{Path, PathBuf};

/// Every place under `crate_dir/src` where production code reaches the disk without the port:
/// `std::fs`, `tokio::fs`, or a bare `fs::` path, as `"<relative file>:<line>: <text>"`.
///
/// Test code is not production code: a `#[cfg(test)]` item (a `mod tests { … }` included) is
/// skipped, as are comments, so a migrated crate's own tests may keep using a temp directory.
/// `build.rs` is outside `src/` and never scanned.
#[must_use]
pub fn direct_disk_io_in(crate_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_rs_files(&crate_dir.join("src"), &mut files);
    files.sort();
    let mut hits = Vec::new();
    for file in files {
        let rel = file.strip_prefix(crate_dir).unwrap_or(&file);
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        scan_file(rel, &text, &mut hits);
    }
    hits
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type` does not follow symlinks, so a linked directory can neither loop the walk
        // nor smuggle a file in twice.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_rs_files(&path, out);
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn scan_file(rel: &Path, text: &str, hits: &mut Vec<String>) {
    let mut lexer = Lexer::default();
    let mut skipping_test_item = false;
    let mut skip_saw_brace = false;
    let mut skip_depth: i32 = 0;
    for (index, line) in text.lines().enumerate() {
        let (code, scan) = lexer.split_line(line);
        // Structure (attributes, braces) is read with strings aside — a `"{"` in a message must
        // not unbalance the skipped item — while the disk patterns match the code as written.
        let flat: String = scan.chars().filter(|c| !c.is_whitespace()).collect();
        if flat.contains("#![cfg(test)]") {
            // The whole file is test code; nothing below can be a hit.
            return;
        }
        if flat.contains("#[cfg(test)]") {
            skipping_test_item = true;
            skip_saw_brace = false;
            skip_depth = 0;
        }
        if skipping_test_item {
            let mut opens = 0;
            let mut closes = 0;
            for b in scan.bytes() {
                match b {
                    b'{' => opens += 1,
                    b'}' => closes += 1,
                    _ => {}
                }
            }
            if opens > 0 {
                skip_saw_brace = true;
            }
            skip_depth += opens - closes;
            // A braced item ends when its braces balance; a braceless one (`use x;`, `mod x;`)
            // ends at its semicolon. Attribute and blank lines between the `cfg` and the item
            // belong to the item.
            if (skip_saw_brace && skip_depth <= 0) || (!skip_saw_brace && scan.contains(';')) {
                skipping_test_item = false;
            }
            continue;
        }
        if reaches_disk(&code) {
            hits.push(format!("{}:{}: {}", rel.display(), index + 1, code.trim()));
        }
    }
}

/// A line lexer that knows comments from strings: `//` inside `"http://…"` ends nothing, a `"{"`
/// in a message opens nothing, and a block comment or raw string stays open past the line end.
#[derive(Default)]
struct Lexer {
    in_block_comment: bool,
    in_string: bool,
    in_raw_string: Option<usize>,
}

impl Lexer {
    /// The line twice: `code` without its comments (strings verbatim), and `scan` with its string
    /// and character literals blanked for brace and attribute detection.
    fn split_line(&mut self, line: &str) -> (String, String) {
        let mut code = String::with_capacity(line.len());
        let mut scan = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        let mut mode = if self.in_block_comment {
            Mode::Block
        } else if let Some(hashes) = self.in_raw_string {
            Mode::Raw(hashes)
        } else if self.in_string {
            Mode::Str
        } else {
            Mode::Normal
        };
        // A string the line ends inside continues only past a `\`-newline; anything else ends it,
        // so one misread quote costs one line rather than the rest of the file.
        let mut continued_string = false;
        while let Some(c) = chars.next() {
            match mode {
                Mode::Block => {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        mode = Mode::Normal;
                    }
                }
                Mode::Str => {
                    code.push(c);
                    scan.push(' ');
                    if c == '\\' {
                        match chars.next() {
                            Some(escaped) => {
                                code.push(escaped);
                                scan.push(' ');
                            }
                            None => continued_string = true,
                        }
                    } else if c == '"' {
                        mode = Mode::Normal;
                    }
                }
                Mode::Raw(hashes) => {
                    code.push(c);
                    scan.push(' ');
                    if c == '"' && raw_closes_here(&chars, hashes) {
                        for _ in 0..hashes {
                            chars.next();
                            code.push('#');
                            scan.push(' ');
                        }
                        mode = Mode::Normal;
                    }
                }
                Mode::Char => {
                    code.push(c);
                    scan.push(' ');
                    if c == '\\' {
                        if let Some(escaped) = chars.next() {
                            code.push(escaped);
                            scan.push(' ');
                        }
                    } else if c == '\'' {
                        mode = Mode::Normal;
                    }
                }
                Mode::Normal => {
                    if c == '/' && chars.peek() == Some(&'/') {
                        break;
                    } else if c == '/' && chars.peek() == Some(&'*') {
                        chars.next();
                        mode = Mode::Block;
                    } else if c == '"' {
                        code.push(c);
                        scan.push(' ');
                        mode = Mode::Str;
                    } else if c == '\'' && is_char_literal_ahead(&chars) {
                        code.push(c);
                        scan.push(' ');
                        mode = Mode::Char;
                    } else if c == 'r' && raw_opens_here(&chars) {
                        code.push(c);
                        scan.push(' ');
                        let mut hashes = 0;
                        while chars.peek() == Some(&'#') {
                            chars.next();
                            hashes += 1;
                            code.push('#');
                            scan.push(' ');
                        }
                        chars.next();
                        code.push('"');
                        scan.push(' ');
                        mode = Mode::Raw(hashes);
                    } else {
                        code.push(c);
                        scan.push(c);
                    }
                }
            }
        }
        self.in_block_comment = mode == Mode::Block;
        self.in_string = mode == Mode::Str && continued_string;
        self.in_raw_string = match mode {
            Mode::Raw(hashes) => Some(hashes),
            _ => None,
        };
        (code, scan)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Mode {
    Normal,
    Block,
    Str,
    Raw(usize),
    Char,
}

/// Whether `"` plus `hashes` hashes close the raw string here (the first valid closer wins).
fn raw_closes_here(chars: &std::iter::Peekable<std::str::Chars<'_>>, hashes: usize) -> bool {
    let mut ahead = chars.clone();
    for _ in 0..hashes {
        if ahead.next() != Some('#') {
            return false;
        }
    }
    true
}

/// Whether `r` starts a raw string (`r"…"`, `r#"…"#`) rather than a raw identifier (`r#type`).
fn raw_opens_here(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut ahead = chars.clone();
    match ahead.peek() {
        Some('"') => true,
        Some('#') => {
            while ahead.peek() == Some(&'#') {
                ahead.next();
            }
            ahead.peek() == Some(&'"')
        }
        _ => false,
    }
}

/// Whether `'` opens a character literal rather than a lifetime: exactly one character or escape
/// before the closing quote (`'\''` and `'\u{1F600}'` included). Anything else — `&'a str`,
/// `Foo<'a, 'b>` — is lifetimes, left for the brace count to see through.
fn is_char_literal_ahead(chars: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut ahead = chars.clone();
    let Some(first) = ahead.next() else {
        return false;
    };
    if first != '\\' {
        return matches!(ahead.next(), Some('\''));
    }
    let Some(escape) = ahead.next() else {
        return false;
    };
    match escape {
        '\'' | '"' | '\\' | 'n' | 'r' | 't' | '0' => {}
        'x' => {
            for _ in 0..2 {
                if !ahead.next().is_some_and(|c| c.is_ascii_hexdigit()) {
                    return false;
                }
            }
        }
        'u' => {
            if ahead.next() != Some('{') {
                return false;
            }
            let mut digits = 0;
            for c in ahead.by_ref() {
                if c == '}' {
                    break;
                }
                if c == '_' {
                    continue;
                }
                if !c.is_ascii_hexdigit() {
                    return false;
                }
                digits += 1;
            }
            if !(1..=6).contains(&digits) {
                return false;
            }
        }
        _ => return false,
    }
    ahead.next() == Some('\'')
}

/// Whether the (comment-free) line names `std::fs`, `tokio::fs`, or a bare `fs::` path. Each name
/// must stand on its own: `tddy_fs::Fs` and `(&dyn Fs)` are the port, not the disk.
fn reaches_disk(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(after) = matches_name(bytes, i, b"std::fs") {
            if is_boundary(bytes, after) {
                return true;
            }
            i = after;
        } else if let Some(after) = matches_name(bytes, i, b"tokio::fs") {
            if is_boundary(bytes, after) {
                return true;
            }
            i = after;
        } else if matches_name(bytes, i, b"fs::").is_some() {
            return true;
        } else {
            i += 1;
        }
    }
    false
}

/// If `name` starts at `at` on a word boundary, the index just past it.
fn matches_name(haystack: &[u8], at: usize, name: &[u8]) -> Option<usize> {
    if at > 0 && is_ident(haystack[at - 1]) {
        return None;
    }
    if haystack.len() >= at + name.len() && haystack[at..at + name.len()] == *name {
        Some(at + name.len())
    } else {
        None
    }
}

/// The name ends where an identifier cannot continue: `std::fs;` and `std::fs::read` match,
/// `std::filesystem` does not.
fn is_boundary(haystack: &[u8], at: usize) -> bool {
    at >= haystack.len() || !is_ident(haystack[at])
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
