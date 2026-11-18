//! Document helpers LawCraft needs without patching `wordcraft-doc`.

use wordcraft_doc::{Document, StoryRef};

/// Every text-bearing story: the body, then each header, footer, footnote, endnote, comment
/// and text box, by part id.
pub fn text_stories(doc: &Document) -> Vec<StoryRef> {
    std::iter::once(StoryRef::Body).chain(doc.parts.keys().map(|k| StoryRef::Part(*k))).collect()
}
