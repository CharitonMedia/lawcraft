//! LawCraft-owned document tests (kept out of WordCraft's tests.rs).

use super::*;

#[test]
fn text_stories_cover_the_body_and_every_part() {
    let mut d = Document::new();
    assert_eq!(d.text_stories(), vec![StoryRef::Body]);
    d.parts.insert(2, Part::default());
    d.parts.insert(1, Part::default());
    assert_eq!(d.text_stories(), vec![StoryRef::Body, StoryRef::Part(1), StoryRef::Part(2)]);
}
