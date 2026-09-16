//! Document synchronisation: which documents this client has told the server about, at which
//! version, and which notification announces a given change.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::json;

use super::{LspClient, LspError};

/// The last version each open document was announced at. A URI's presence is what "open" means:
/// `did_open` inserts it, `did_close` removes it, and `did_change` refuses a URI that is absent.
pub(super) type DocumentVersions = Mutex<HashMap<String, i64>>;

/// The version a freshly opened document is announced at.
const FIRST_DOCUMENT_VERSION: i64 = 1;

impl LspClient {
    /// Open a source file as an LSP document so the server indexes it.
    ///
    /// Announces version 1 whether or not the URI was already open, so a caller that may be
    /// re-announcing a document the server already holds wants [`Self::sync_document`] instead.
    pub async fn did_open(&self, uri: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        self.documents
            .lock()
            .unwrap()
            .insert(uri.to_string(), FIRST_DOCUMENT_VERSION);
        self.send_did_open(uri, language_id, text)
    }

    /// Announce `text` as the current contents of `uri`: an open for a document this client has
    /// not opened, an edit at the next version for one it has.
    ///
    /// What a host that outlives one request needs in place of a re-open. Opening an already-open
    /// document announces version 1 again, and a server that has advanced that document past 1 is
    /// entitled to ignore an edit whose version goes backwards — so a re-announcement would stop
    /// reaching the server silently rather than fail.
    pub async fn sync_document(
        &self,
        uri: &str,
        language_id: &str,
        text: &str,
    ) -> Result<(), LspError> {
        // Decided under one lock: two callers announcing the same new URI must not both conclude
        // it is theirs to open.
        let version = {
            let mut documents = self.documents.lock().unwrap();
            match documents.get_mut(uri) {
                Some(version) => {
                    *version += 1;
                    Some(*version)
                }
                None => {
                    documents.insert(uri.to_string(), FIRST_DOCUMENT_VERSION);
                    None
                }
            }
        };
        match version {
            Some(version) => self.send_did_change(uri, version, text),
            None => self.send_did_open(uri, language_id, text),
        }
    }

    /// Replace an open document's contents, advancing the version the server is told about.
    ///
    /// The version counter belongs here rather than to a caller because a server tracks versions
    /// per document for its whole life, while a caller that drives one operation does not: a second
    /// caller starting again at 1 against a server that has already seen 40 is a protocol
    /// violation, and the server is entitled to ignore the edit. Refused for a URI this client has
    /// not opened, since there is no version sequence to continue.
    pub async fn did_change(&self, uri: &str, text: &str) -> Result<(), LspError> {
        let version = {
            let mut documents = self.documents.lock().unwrap();
            let version = documents
                .get_mut(uri)
                .ok_or_else(|| LspError::DocumentNotOpen(uri.to_string()))?;
            *version += 1;
            *version
        };
        self.send_did_change(uri, version, text)
    }

    /// Close an open document, so a later reopen legitimately starts its versions again.
    ///
    /// Refused for a URI this client has not opened, for the same reason [`Self::did_change`] is.
    pub async fn did_close(&self, uri: &str) -> Result<(), LspError> {
        if self.documents.lock().unwrap().remove(uri).is_none() {
            return Err(LspError::DocumentNotOpen(uri.to_string()));
        }
        // `DidCloseTextDocumentParams` carries a bare document identifier: a close ends the
        // version sequence rather than extending it.
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    /// Announce a document as newly open at [`FIRST_DOCUMENT_VERSION`].
    fn send_did_open(&self, uri: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": FIRST_DOCUMENT_VERSION,
                    "text": text,
                }
            }),
        )
    }

    /// Announce an open document's whole contents at `version`.
    fn send_did_change(&self, uri: &str, version: i64, text: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                // A full-text change: the whole document replaces the whole document. The
                // incremental form would need the client to track ranges the caller never sends.
                "contentChanges": [{ "text": text }],
            }),
        )
    }
}
