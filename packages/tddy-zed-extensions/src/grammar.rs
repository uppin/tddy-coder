//! Grammars from `grammars/<name>.wasm`, loaded through tree-sitter's WASM loader.
//!
//! One wasmtime engine and one `WasmStore` serve the whole process: a WASM language can be used
//! only by a parser holding the store that loaded it, so the store lives inside the one
//! [`tree_sitter::Parser`] this holds, and every grammar is loaded once into it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tree_sitter::{Language, Parser, Tree, WasmError, WasmErrorKind, WasmStore};

/// A grammar that did not load (R8): reported once per run, naming the extension, the grammar and
/// why — `extension <id>: grammar <g> did not load: <reason>`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("extension {extension}: grammar {grammar} did not load: {reason}")]
pub struct GrammarLoadError {
    pub extension: String,
    pub grammar: String,
    pub reason: String,
}

/// The process's grammars: one engine, one store, each grammar loaded once.
pub struct GrammarStore {
    /// The parser that owns the process's `WasmStore`.
    parser: Parser,
    /// Grammars already loaded, by `(extension dir, grammar name)`.
    loaded: BTreeMap<(PathBuf, String), Language>,
}

impl GrammarStore {
    /// A store with no grammar loaded: a fresh wasmtime engine and the `WasmStore` over it, set on
    /// the parser this store parses with.
    pub fn new() -> Result<Self, tree_sitter::WasmError> {
        let engine = tree_sitter::wasmtime::Engine::default();
        let store = WasmStore::new(&engine)?;
        let mut parser = Parser::new();
        parser.set_wasm_store(store).map_err(|error| WasmError {
            kind: WasmErrorKind::Other,
            message: error.to_string(),
        })?;
        Ok(Self {
            parser,
            loaded: BTreeMap::new(),
        })
    }

    /// The grammar `grammar` of the extension `extension` installed at `extension_dir`, from its
    /// `grammars/<grammar>.wasm`; loaded on the first call, the same [`Language`] after.
    pub fn load(
        &mut self,
        extension: &str,
        extension_dir: &Path,
        grammar: &str,
    ) -> Result<Language, GrammarLoadError> {
        let key = (extension_dir.to_path_buf(), grammar.to_string());
        if let Some(language) = self.loaded.get(&key) {
            return Ok(language.clone());
        }
        let failed = |reason: String| GrammarLoadError {
            extension: extension.to_string(),
            grammar: grammar.to_string(),
            reason,
        };
        let file = extension_dir
            .join("grammars")
            .join(format!("{grammar}.wasm"));
        let bytes =
            std::fs::read(&file).map_err(|error| failed(format!("{}: {error}", file.display())))?;

        // The parser owns the store; it is taken out to load into and put back whatever came of it.
        let mut store = self
            .parser
            .take_wasm_store()
            .ok_or_else(|| failed("the parser holds no WASM store".to_string()))?;
        let loaded = store.load_language(grammar, &bytes);
        self.parser
            .set_wasm_store(store)
            .map_err(|error| failed(error.to_string()))?;
        let language = loaded.map_err(|error| failed(error.to_string()))?;

        self.loaded.insert(key, language.clone());
        Ok(language)
    }

    /// `text` parsed with `language`, which this store loaded.
    pub fn parse(&mut self, language: &Language, text: &str) -> Option<Tree> {
        self.parser.set_language(language).ok()?;
        self.parser.parse(text, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The vendored tree-sitter-toml grammar's directory, laid out as an installed extension's
    /// `grammars/` would be one level down: `<dir>/grammars/toml.wasm`.
    fn an_extension_dir_with_the_toml_grammar() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("temp extension dir");
        std::fs::create_dir_all(dir.path().join("grammars")).expect("grammars dir");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/grammars/tree-sitter-toml/toml.wasm"),
            dir.path().join("grammars/toml.wasm"),
        )
        .expect("copy the vendored grammar");
        dir
    }

    #[test]
    fn the_vendored_grammar_loads_and_parses() {
        // Given a store and an extension dir shipping the TOML grammar
        let dir = an_extension_dir_with_the_toml_grammar();
        let mut store = GrammarStore::new().expect("a store");

        // When the grammar is loaded and a table parsed with it
        let language = store
            .load("demo-toml", dir.path(), "toml")
            .expect("it loads");
        let tree = store
            .parse(&language, "[server]\nport = 8080\n")
            .expect("it parses");

        // Then the parse is TOML's
        assert_eq!(
            tree.root_node().to_sexp(),
            "(document (table (bare_key) (pair (bare_key) (integer))))"
        );
    }

    #[test]
    fn a_grammar_is_loaded_once_per_store() {
        // Given a store that has loaded the TOML grammar
        let dir = an_extension_dir_with_the_toml_grammar();
        let mut store = GrammarStore::new().expect("a store");
        let first = store
            .load("demo-toml", dir.path(), "toml")
            .expect("it loads");

        // When the grammar's file is gone and it is asked for again
        std::fs::remove_file(dir.path().join("grammars/toml.wasm")).expect("remove the grammar");
        let second = store.load("demo-toml", dir.path(), "toml");

        // Then the language already loaded answers, without reading the file again
        assert_eq!(
            second.map(|language| language.name().map(str::to_string)),
            Ok(first.name().map(str::to_string))
        );
    }

    #[test]
    fn a_missing_grammar_file_names_its_extension_and_grammar() {
        // Given an extension dir with no `grammars/absent.wasm`
        let dir = tempfile::TempDir::new().expect("temp extension dir");
        let mut store = GrammarStore::new().expect("a store");

        // When that grammar is loaded
        let refused = store.load("an-extension", dir.path(), "absent");

        // Then the refusal names the extension and the grammar
        assert_eq!(
            refused
                .map_err(|error| (error.extension, error.grammar))
                .err(),
            Some(("an-extension".to_string(), "absent".to_string()))
        );
    }
}
