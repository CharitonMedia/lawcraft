//! Text-searchability verdicts for LawCraft PDF export checks.
//!
//! Kept out of WordCraft's [`super`] lib root so the searchability API stays in one place.

#[cfg(not(target_arch = "wasm32"))]
use super::extract;

/// What a build could learn about a PDF's text layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSearchability {
    /// The bytes carry extractable document text.
    Searchable,
    /// The bytes are a PDF with no extractable document text and no page-dominating picture (for
    /// example, a font-only PDF). An image-only page is [`TextSearchability::Uncertain`], not this.
    NotSearchable,
    /// At least one page draws a picture that dominates it, so the page's own text (if any) cannot
    /// verify the picture's content: coverage cannot be verified. This is **not** the same as
    /// success.
    Uncertain,
    /// This build cannot inspect PDF text at all (wasm has no PDF interpreter); verification is
    /// unavailable, which is **not** the same as success.
    Unavailable,
}

impl TextSearchability {
    /// `Some(true)`/`Some(false)` when the PDF was actually inspected and the answer is definite,
    /// `None` when it could not be verified (an uncertain page, or a build with no extractor).
    pub fn is_searchable(self) -> Option<bool> {
        match self {
            TextSearchability::Searchable => Some(true),
            TextSearchability::NotSearchable => Some(false),
            TextSearchability::Uncertain | TextSearchability::Unavailable => None,
        }
    }
}

/// Judge a PDF from the result of an extraction attempt: `None` is a build with no extractor.
///
/// Kept separate from the `cfg`-gated extraction so the "no extractor" arm is covered off wasm
/// too, and so a bare `%PDF` signature can never be mistaken for extracted text.
pub fn searchability_verdict(extracted_text: Option<bool>) -> TextSearchability {
    match extracted_text {
        Some(true) => TextSearchability::Searchable,
        Some(false) => TextSearchability::NotSearchable,
        None => TextSearchability::Unavailable,
    }
}

/// Whether `bytes` is a PDF with searchable text, judged page by page.
#[cfg(not(target_arch = "wasm32"))]
pub fn text_searchability(bytes: &[u8]) -> TextSearchability {
    if bytes.len() < 8 || !bytes.starts_with(b"%PDF") {
        return TextSearchability::NotSearchable;
    }
    match extract::extract_verdict(bytes) {
        extract::Verdict::Searchable => TextSearchability::Searchable,
        extract::Verdict::NotSearchable => TextSearchability::NotSearchable,
        extract::Verdict::Uncertain => TextSearchability::Uncertain,
    }
}

/// Always [`TextSearchability::Unavailable`]: the web build can't read PDFs.
#[cfg(target_arch = "wasm32")]
pub fn text_searchability(_bytes: &[u8]) -> TextSearchability {
    searchability_verdict(None)
}
