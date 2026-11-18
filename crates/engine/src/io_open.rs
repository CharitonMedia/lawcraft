//! PDF-aware open reports and the LawCraft docx prefix hook.
//!
//! Kept beside WordCraft's io core so PDF import and appendix page-number prefixes stay out of that file.

use wordcraft_doc::Document;

#[cfg(not(target_arch = "wasm32"))]
use super::{LocalImages, fit_pictures, opened, read_document};
use super::{ext_of, load_embedded_fonts, read_bytes, region_page};

/// Result of opening a file, including conversion details for formats that must be saved as a copy.
pub struct OpenedDocument {
    pub doc: Document,
    pub encrypted: bool,
    pub pdf: Option<wordcraft_pdf::import::ImportSummary>,
}

/// After a Word package save, rewrite reserved appendix page-number prefixes into `w:docVars`.
pub(super) fn maybe_write_prefixes(ext: &str, mut bytes: Vec<u8>, doc: &Document) -> Result<Vec<u8>, String> {
    if wordcraft_docx::Flavor::from_ext(ext).is_some() {
        wordcraft_law_docx::write_page_num_prefixes(&mut bytes, doc).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

/// Open a file and report a PDF conversion separately from lossless document opening.
pub fn open_bytes_report(name: &str, bytes: &[u8], password: Option<&str>) -> Result<OpenedDocument, String> {
    open_bytes_report_in(name, bytes, password, &wordcraft_geom::Region::default())
}

/// [`open_bytes_report`] using the region's paper for formats without a page size.
pub fn open_bytes_report_in(name: &str, bytes: &[u8], password: Option<&str>, region: &wordcraft_geom::Region) -> Result<OpenedDocument, String> {
    if wordcraft_pdf::import::is_pdf(bytes) || ext_of(name) == "pdf" {
        let (doc, summary) = wordcraft_pdf::import::read(bytes, password).map_err(|e| match e {
            wordcraft_pdf::import::ImportError::NeedsPassword => wordcraft_docx::PASSWORD_REQUIRED.to_string(),
            wordcraft_pdf::import::ImportError::WrongPassword => wordcraft_docx::WRONG_PASSWORD.to_string(),
            other => other.to_string(),
        })?;
        return Ok(OpenedDocument { doc, encrypted: false, pdf: Some(summary) });
    }
    let (doc, encrypted) = read_bytes(name, bytes, password, &region_page(region))?;
    load_embedded_fonts(&doc);
    Ok(OpenedDocument { doc, encrypted, pdf: None })
}

/// File-system variant of [`open_bytes_report`], with the same automation read confinement.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path_report(path: &std::path::Path, password: Option<&str>) -> Result<OpenedDocument, String> {
    open_path_report_in(path, password, &wordcraft_geom::Region::default())
}

/// [`open_path_report`] using the region's paper for formats without a page size. The file is
/// read as [`read_document`] reads it; a PDF must also be within the import limit.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path_report_in(path: &std::path::Path, password: Option<&str>, region: &wordcraft_geom::Region) -> Result<OpenedDocument, String> {
    let bytes = read_document(path)?;
    let name = path.to_string_lossy();
    let pdf = wordcraft_pdf::import::is_pdf(&bytes);
    if (pdf || ext_of(&name) == "pdf") && bytes.len() > wordcraft_pdf::import::MAX_BYTES {
        return Err(format!("{}: file exceeds the {} MiB import limit", path.display(), wordcraft_pdf::import::MAX_BYTES / (1024 * 1024)));
    }
    if !pdf && matches!(ext_of(&name).as_str(), "html" | "htm" | "xhtml") {
        // Pictures referenced by a relative path load from beside the HTML file.
        let images = LocalImages::new(path.parent().unwrap_or(std::path::Path::new("")));
        let html = wordcraft_formats::html::decode(&bytes);
        let mut doc = wordcraft_formats::html::import_on(&html, &|src| images.load(src), &region_page(region));
        fit_pictures(&mut doc);
        opened(&mut doc);
        return Ok(OpenedDocument { doc, encrypted: false, pdf: None });
    }
    open_bytes_report_in(&name, &bytes, password, region)
}

/// Web callers supply bytes through [`open_bytes_report`].
#[cfg(target_arch = "wasm32")]
pub fn open_path_report(_path: &std::path::Path, _password: Option<&str>) -> Result<OpenedDocument, String> {
    Err("file-system opening is unavailable in the browser; supply file bytes".into())
}

/// Region-aware path opening has the same browser limitation as [`open_path_report`].
#[cfg(target_arch = "wasm32")]
pub fn open_path_report_in(path: &std::path::Path, password: Option<&str>, _region: &wordcraft_geom::Region) -> Result<OpenedDocument, String> {
    open_path_report(path, password)
}

/// Read a Word package (decrypted with `password` if it is encrypted), then apply LawCraft appendix
/// page-number prefixes from `w:docVars`.
pub(super) fn read_docx(
    bytes: &[u8],
    password: Option<&str>,
    chunks: Option<&dyn Fn(&wordcraft_docx::AltChunk) -> Option<Document>>,
) -> Result<Document, wordcraft_docx::DocxError> {
    // Decrypt once so the LawCraft prefix hook sees the same OOXML package bytes the reader used.
    let decrypted;
    let package: &[u8] = if wordcraft_docx::is_encrypted(bytes) {
        decrypted = wordcraft_docx::decrypt(bytes, password)?;
        decrypted.as_slice()
    } else {
        bytes
    };
    let mut doc = wordcraft_docx::read_with(package, &wordcraft_docx::ReadOptions { password: None, chunks })?;
    wordcraft_law_docx::apply_page_num_prefixes(&mut doc, package).map_err(|e| wordcraft_docx::DocxError::Zip(e.to_string()))?;
    Ok(doc)
}
