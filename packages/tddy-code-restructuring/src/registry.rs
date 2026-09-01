//! Language dispatch.
//!
//! Adding a language is one [`LanguageBackend`] implementation plus one registration. The
//! executor, ledger, journal, and plan parser never change — that is the open-closed boundary.

use crate::edit::Resolution;
use crate::overlay::Overlay;
use crate::plan::{RefactorKind, RefactorOp};
use crate::Result;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
}

/// The workspace a backend resolves against.
///
/// A backend reads source through the overlay rather than straight from disk, so that a run which
/// resolves without writing still shows each operation the text its predecessors produced. On a real
/// run the overlay is empty and every read reaches the tree being edited.
pub struct Workspace<'a> {
    pub root: &'a Path,
    pub overlay: &'a Overlay,
}

impl<'a> Workspace<'a> {
    /// The current contents of a path the plan named, relative to the root.
    pub fn read(&self, relative: &str) -> Result<String> {
        self.overlay.read(self.root, Path::new(relative))
    }
}

pub trait LanguageBackend {
    fn language(&self) -> Language;

    /// Extensions this backend claims, without the leading dot.
    fn handles_extension(&self, extension: &str) -> bool;

    /// Whether this backend can resolve the given operation.
    fn supports(&self, kind: RefactorKind) -> bool;

    /// Resolve one intent into a concrete, multi-file edit by asking the language's own
    /// refactoring engine. This is where code text originates.
    ///
    /// The [`Resolution`] carries the edit plus anything the backend has to report about it, so a
    /// consequence the operation could not avoid — a visibility it had to widen — reaches the journal
    /// instead of being visible only in the diff.
    fn resolve(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution>;

    /// What is wrong with an operation that can be judged from the text alone, without a language
    /// server and without writing anything.
    ///
    /// Every anchor in a plan is written in the coordinates of the snapshot, so a check that reads
    /// only the original text needs no simulation of the operations before it. That is what makes this
    /// tier worth having separately: a name collision or a split attribute is a lexical fact, and
    /// paying a cold index to be told about it is the cost the preflight exists to avoid.
    ///
    /// Returns every finding rather than the first, because a plan is checked to be fixed in one pass.
    /// Default-empty so a backend with nothing statically checkable is unaffected.
    fn check(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>> {
        let _ = (op, workspace);
        Ok(Vec::new())
    }

    /// The range anchor covering a named, adjacent run of items, trivia included.
    ///
    /// Hand-computing a seam's extent is the busywork whose failures look like tool bugs: "start
    /// marker, then the next item's marker minus two" breaks when adjacent seams destroy each other's
    /// markers, and again when one doc comment is a prefix of another. The engine already knows where
    /// items begin and end, so it is asked.
    fn anchor_for(
        &mut self,
        file: &str,
        items: &[String],
        workspace: &Workspace<'_>,
    ) -> Result<crate::edit::Range> {
        let _ = (file, items, workspace);
        Err(crate::RestructureError::UnsupportedOp {
            backend: format!("{:?}", self.language()),
            op: "anchors".to_string(),
        })
    }
}

#[derive(Default)]
pub struct BackendRegistry {
    backends: Vec<Box<dyn LanguageBackend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, backend: Box<dyn LanguageBackend>) {
        self.backends.push(backend);
    }

    /// Find the backend for a file and confirm it supports the operation.
    ///
    /// An unsupported operation is an error, never a silent skip: skipping it would leave the tree
    /// in a state the rest of the plan was not written against.
    pub fn backend_for(
        &mut self,
        path: &Path,
        kind: RefactorKind,
    ) -> Result<&mut dyn LanguageBackend> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();

        let index = self
            .backends
            .iter()
            .position(|backend| backend.handles_extension(extension))
            .ok_or_else(|| crate::RestructureError::NoBackend {
                extension: extension.to_string(),
            })?;

        let backend = &mut self.backends[index];
        if !backend.supports(kind) {
            return Err(crate::RestructureError::UnsupportedOp {
                backend: format!("{:?}", backend.language()),
                op: format!("{kind:?}"),
            });
        }
        Ok(backend.as_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Resolution;
    use crate::plan::{Anchor, RefactorOp};
    use crate::RestructureError;
    use std::path::PathBuf;

    struct StubBackend {
        language: Language,
        extension: &'static str,
        supported: Vec<RefactorKind>,
    }

    impl LanguageBackend for StubBackend {
        fn language(&self) -> Language {
            self.language
        }

        fn handles_extension(&self, extension: &str) -> bool {
            extension == self.extension
        }

        fn supports(&self, kind: RefactorKind) -> bool {
            self.supported.contains(&kind)
        }

        fn resolve(
            &mut self,
            _op: &RefactorOp,
            _workspace: &Workspace<'_>,
        ) -> crate::Result<Resolution> {
            Ok(Resolution::default())
        }
    }

    fn registry() -> BackendRegistry {
        let mut registry = BackendRegistry::new();
        registry.register(Box::new(StubBackend {
            language: Language::Rust,
            extension: "rs",
            supported: vec![
                RefactorKind::ExtractMethod,
                RefactorKind::ExtractModuleToFile,
            ],
        }));
        registry
    }

    #[test]
    fn routes_a_rust_file_to_the_rust_backend() {
        let mut registry = registry();

        let backend = registry
            .backend_for(&PathBuf::from("src/lib.rs"), RefactorKind::ExtractMethod)
            .unwrap();

        assert_eq!(backend.language(), Language::Rust);
    }

    #[test]
    fn reports_the_extension_when_no_backend_claims_the_file() {
        let mut registry = registry();

        let outcome =
            registry.backend_for(&PathBuf::from("style.css"), RefactorKind::ExtractMethod);

        match outcome {
            Err(RestructureError::NoBackend { extension }) => assert_eq!(extension, "css"),
            other => panic!("expected NoBackend, got {:?}", other.err()),
        }
    }

    /// An operation a backend cannot perform is a hard error. Skipping it would leave the tree in
    /// a state the rest of the plan was not written against.
    #[test]
    fn refuses_an_operation_the_matching_backend_does_not_support() {
        let mut registry = registry();

        let outcome = registry.backend_for(&PathBuf::from("src/lib.rs"), RefactorKind::MoveSymbol);

        match outcome {
            Err(RestructureError::UnsupportedOp { backend, op }) => {
                assert!(backend.to_lowercase().contains("rust"));
                assert!(op.to_lowercase().contains("move"));
            }
            other => panic!("expected UnsupportedOp, got {:?}", other.err()),
        }
    }

    /// Open-closed: registering another backend must not disturb routing to the one already there.
    #[test]
    fn registering_another_rust_backend_leaves_the_first_dispatch_unchanged() {
        let mut registry = registry();
        registry.register(Box::new(StubBackend {
            language: Language::Rust,
            extension: "rs",
            supported: vec![RefactorKind::RenameSymbol],
        }));

        let backend = registry
            .backend_for(&PathBuf::from("src/lib.rs"), RefactorKind::ExtractMethod)
            .unwrap();

        assert_eq!(backend.language(), Language::Rust);
    }

    #[test]
    fn resolves_an_operation_through_the_backend_it_routed_to() {
        let mut registry = registry();
        let op = RefactorOp {
            op: RefactorKind::ExtractMethod,
            anchor: Anchor::Symbol {
                file: "src/lib.rs".to_string(),
                path: "helper".to_string(),
            },
            name: Some("extracted".to_string()),
            to: None,
            variant: None,
            with_private_deps: true,
            reexport: None,
            to_file: false,
        };
        let root = PathBuf::from("/tmp/workspace");

        let backend = registry
            .backend_for(&PathBuf::from("src/lib.rs"), RefactorKind::ExtractMethod)
            .unwrap();
        let overlay = crate::Overlay::new();
        let edit = backend
            .resolve(
                &op,
                &Workspace {
                    root: &root,
                    overlay: &overlay,
                },
            )
            .unwrap();

        assert_eq!(edit, Resolution::default());
    }
}
