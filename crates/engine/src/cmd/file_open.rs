//! LawCraft file-open helpers: PDF import reports and export-check findings.
//!
//! Kept beside WordCraft's [`super::file`] so PDF open / export-check wiring stays out of that core.

use serde_json::{Value, json};
use wordcraft_doc::Document;

use super::file::normal_document;
use crate::io::{ExportOptions, OpenedDocument};
use crate::{CmdError, CmdResult, Session, p};

/// Open `path` (or base64 `data`) through the report APIs so a PDF yields an editable import summary.
pub(super) fn open_reported(s: &Session, path: &str, v: &Value) -> Result<OpenedDocument, CmdError> {
    let password = p::str(v, "password");
    // An empty file opens as a new blank document that saves back to it (#1384). PDF opens go
    // through the report APIs so LawCraft can import a reflowed editable copy.
    if let Some(data) = p::str(v, "data") {
        let bytes = super::insert::base64_decode(data).ok_or_else(|| CmdError::Params("bad base64".into()))?;
        if bytes.is_empty() {
            return Ok(OpenedDocument { doc: normal_document(s), encrypted: false, pdf: None });
        }
        return crate::io::open_bytes_report_in(path, &bytes, password, &s.region()).map_err(CmdError::Failed);
    }
    if crate::io::is_empty_file(std::path::Path::new(path)) {
        return Ok(OpenedDocument { doc: normal_document(s), encrypted: false, pdf: None });
    }
    crate::io::open_path_report_in(std::path::Path::new(path), password, &s.region()).map_err(CmdError::Failed)
}

/// Apply editing-language defaults for plain text / Markdown after a reported open.
pub(super) fn apply_plain_lang(s: &Session, path: &str, doc: &mut Document) {
    let ext = std::path::Path::new(path).extension().and_then(std::ffi::OsStr::to_str).unwrap_or("").to_ascii_lowercase();
    if let Some(lang) = s.prefs.default_language.as_ref().or(s.editing_lang.as_ref())
        && matches!(ext.as_str(), "txt" | "text" | "md" | "markdown")
    {
        std::sync::Arc::make_mut(&mut doc.styles).default_chr.lang = Some(lang.clone());
    }
}

/// Finish a PDF (or ordinary) open: path, dirty/status for imports, password, and result fields.
pub(super) fn finish_open(
    s: &mut Session,
    path: &str,
    password: Option<&str>,
    encrypted: bool,
    pdf: Option<wordcraft_pdf::import::ImportSummary>,
) -> CmdResult {
    if let Some(z) = s.doc.settings.zoom {
        super::view::restore_zoom(s, z);
    }
    // A reflowed PDF is a new editable document, never an implicit overwrite of the source.
    s.path = if pdf.is_some() { None } else { Some(path.into()) };
    if pdf.is_some() {
        // Save As suggests the PDF's own name, as a Word document.
        s.file_name = std::path::Path::new(path).file_stem().map(|n| n.to_string_lossy().to_string()).filter(|n| !n.is_empty());
        s.dirty = true;
        s.status = "PDF imported as an editable copy.".into();
    }
    // Like Word, a document opened with a password is saved with it again.
    if encrypted {
        s.password = password.map(crate::io::Password::new);
    }
    let mut result = json!({"path": path, "paragraphs": s.doc.paragraph_count(), "words": s.doc.word_count(), "encrypted": encrypted, "alreadyOpen": false, "id": s.document_id()});
    if let Some(summary) = pdf {
        s.push_ui_request(json!({"open": "pdfImportReport", "warnings": summary.warnings}));
        result["import"] = json!({"format": "pdf", "pages": summary.pages, "textCharacters": summary.text_characters, "images": summary.images, "warnings": summary.warnings, "saveAsRequired": true});
    }
    Ok(result)
}

/// Run registered export checks for `format` and return their findings.
pub(super) fn export_findings(s: &mut Session, opts: &ExportOptions, format: &str) -> Vec<Value> {
    s.run_export_checks(opts, format)
}
