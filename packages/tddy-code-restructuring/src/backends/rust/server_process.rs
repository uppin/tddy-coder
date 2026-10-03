use std::path::Path;

use std::process::ChildStdout;

use std::io::BufReader;

use std::process::ChildStdin;

use std::process::Child;

/// Drop `use` declarations left binding nothing at all.
///
/// Moving items out of a file leaves the parent importing what they needed, and where every name in a
/// grouped import moved the assist hollows the group out rather than removing the line —
/// `use std::sync::{};`. An empty group binds nothing, ever, so this needs no evidence from the server
/// and cannot be wrong: it is the one part of the unused-import tail that is decidable by looking.
///
/// Deliberately only the empty group. A `use` that resolves is left alone however unused it looks,
/// because dropping a trait import breaks method resolution invisibly — the same reason
/// [`RustBackend::prune_assist_imports`] is narrow.
pub(crate) fn without_hollow_imports(text: &str) -> String {
    let kept: Vec<&str> = text
        .split('\n')
        .filter(|line| !binds_nothing(line))
        .collect();
    kept.join("\n")
}

/// Whether a line is a `use` whose brace group is empty.
pub(crate) fn binds_nothing(line: &str) -> bool {
    let body = line.trim();
    let body = body.strip_prefix("pub ").unwrap_or(body);
    let Some(rest) = body.strip_prefix("use ") else {
        return false;
    };
    let Some(inner) = rest.strip_suffix(';') else {
        return false;
    };
    let Some(open) = inner.find('{') else {
        return false;
    };

    inner.ends_with('}') && inner[open + 1..inner.len() - 1].trim().is_empty()
}

pub(crate) struct Server {
    pub(crate) process: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: BufReader<ChildStdout>,
}

/// One line naming everything that decides how rust-analyzer resolves std and dependencies.
///
/// A stall at `discovering sysroot` looks the same whether the toolchain was pinned, whether
/// cargo/rustc are real binaries or rustup proxies, and whether rust-src is present. This is
/// what tells them apart in a CI log.
pub(crate) fn describe_server_environment(
    binary: &Path,
    toolchain: &str,
    toolchain_bin: &Path,
) -> String {
    let present = |name: &str| {
        if toolchain_bin.join(name).exists() {
            "real"
        } else {
            "missing"
        }
    };
    let rust_src = toolchain_bin
        .parent()
        .map(|prefix| prefix.join("lib/rustlib/src/rust/library"))
        .is_some_and(|path| path.exists());
    format!(
        "server={}; RUSTUP_TOOLCHAIN={toolchain}; cargo={}; rustc={}; rust-src={}",
        binary.display(),
        present("cargo"),
        present("rustc"),
        if rust_src { "present" } else { "absent" },
    )
}

pub(crate) fn default_toolchain_name(rustup_home: &Path) -> Option<String> {
    std::fs::read_to_string(rustup_home.join("settings.toml"))
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                let rest = line.strip_prefix("default_toolchain")?;
                rest.split('"').nth(1).map(str::to_string)
            })
        })
}
