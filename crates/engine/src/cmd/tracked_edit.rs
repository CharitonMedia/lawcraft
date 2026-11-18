//! Tracked replace helpers for optional modules (LawCraft AutoFormat / certificate text).
//!
//! Kept out of WordCraft's [`super`] command root so module edit helpers stay in one place.

use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Pos, RevisionKind, StoryRef};

use super::{new_revision, track_delete};
use crate::{CmdError, Session};

/// Replace `a..b` (within one paragraph) with `text`, as Track Changes records typing over a
/// selection when it is on: the old text is marked deleted (or removed, when this author inserted
/// it) and the new text marked inserted, so Reject restores the old. With tracking off it is a
/// plain replacement. Modules use this for an automatic correction. Returns the end of the new text.
pub fn replace_text(s: &mut Session, a: &Pos, b: &Pos, text: &str) -> Result<Pos, CmdError> {
    if a.story != b.story || a.path != b.path || b.off < a.off {
        return Err(CmdError::Params("replace_text works within one paragraph".into()));
    }
    let props = s.doc.para(a.story, &a.path).map(|p| p.props_of_char(a.off).clone()).ok_or_else(|| CmdError::Failed("no paragraph there".into()))?;
    if !s.doc.settings.track_changes {
        let p = s.doc.para_mut(a.story, &a.path)?;
        p.delete(a.off, b.off)?;
        p.insert_text(a.off, text, &props)?;
        p.touch();
        return Ok(Pos { off: a.off + text.len(), ..a.clone() });
    }
    // The new text goes in after the old first, so deleting the old can't move it.
    let rid = new_revision(s, RevisionKind::Insert);
    let ins = CharProps { ins: Some(rid), del: None, ..props };
    let p = s.doc.para_mut(a.story, &a.path)?;
    p.insert_text(b.off, text, &ins)?;
    p.touch();
    let before = s.doc.para(a.story, &a.path).map_or(0, |p| p.len());
    track_delete(s, a, b)?;
    let removed = before.saturating_sub(s.doc.para(a.story, &a.path).map_or(0, |p| p.len()));
    Ok(Pos { off: b.off - removed + text.len(), ..a.clone() })
}

/// Replace top-level blocks `from..to` of `story` with `blocks`, as one tracked change when Track
/// Changes is on: the new paragraphs go in marked inserted and the old ones are marked deleted,
/// so Reject All brings the old ones back. With tracking off the old blocks are simply replaced.
pub fn replace_blocks(s: &mut Session, story: StoryRef, from: usize, to: usize, blocks: Vec<wordcraft_doc::Block>) -> Result<(), CmdError> {
    use wordcraft_doc::{Block, Path};
    if !s.doc.settings.track_changes {
        for i in (from..to).rev() {
            s.doc.remove_block(story, &Path::top(i))?;
        }
        for (k, b) in blocks.into_iter().enumerate() {
            s.doc.insert_block(story, &Path::top(from + k), b)?;
        }
        return Ok(());
    }
    let rid = new_revision(s, RevisionKind::Insert);
    let n = blocks.len();
    for (k, mut b) in blocks.into_iter().enumerate() {
        if let Block::Para(p) = &mut b {
            let len = p.len();
            p.format(0, len, &|c| c.ins = Some(rid))?;
            p.mark.ins = Some(rid);
        }
        s.doc.insert_block(story, &Path::top(from + k), b)?;
    }
    if to > from {
        // The old blocks now sit after the new ones. Their paragraph marks go too: the deletion
        // runs to the start of the block after them when there is one.
        let (old_from, old_to) = (from + n, to + n);
        let a = Pos { story, path: Path::top(old_from), off: 0 };
        let b = match s.doc.block(story, &Path::top(old_to)) {
            Some(Block::Para(_)) => Pos { story, path: Path::top(old_to), off: 0 },
            _ => {
                let last = Path::top(old_to - 1);
                let len = s.doc.para(story, &last).map_or(0, |p| p.len());
                Pos { story, path: last, off: len }
            }
        };
        track_delete(s, &a, &b)?;
    }
    Ok(())
}

#[cfg(test)]
mod tracked_edit_tests {
    use serde_json::json;
    use wordcraft_doc::{Block, Document, Paragraph, Path, Pos, StoryRef};

    use super::{replace_blocks, replace_text};
    use crate::Session;

    fn body(s: &Session) -> String {
        s.doc.plain_text(StoryRef::Body)
    }

    fn at(off: usize) -> Pos {
        Pos { story: StoryRef::Body, path: Path::top(0), off }
    }

    /// A module's automatic correction is a tracked change while Track Changes is on: Reject All
    /// brings the old text back, Accept All keeps the new.
    #[test]
    fn replace_text_is_tracked_when_tracking_is_on() {
        for (answer, want) in [("review.rejectAll", "Section 1"), ("review.acceptAll", "Section\u{a0}1")] {
            let mut s = Session::new(Document::from_text("Section 1"));
            s.run("review.trackChanges", &json!({"value": true})).unwrap();
            replace_text(&mut s, &at(7), &at(8), "\u{a0}").unwrap();
            assert!(!s.doc.revisions.is_empty(), "recorded as revisions");
            s.run(answer, &json!({})).unwrap();
            assert_eq!(body(&s), want, "{answer}");
        }
        let mut s = Session::new(Document::from_text("Section 1"));
        replace_text(&mut s, &at(7), &at(8), "\u{a0}").unwrap();
        assert_eq!(body(&s), "Section\u{a0}1");
        assert!(s.doc.revisions.is_empty(), "untracked without Track Changes");
    }

    /// Replacing whole paragraphs is one tracked change: Reject All restores the old ones.
    #[test]
    fn replace_blocks_is_tracked_when_tracking_is_on() {
        let new = || {
            vec![Block::Para(Paragraph::with_text("New one", Default::default())), Block::Para(Paragraph::with_text("New two", Default::default()))]
        };
        for (answer, want) in [("review.rejectAll", "Heading\nOld one\nOld two\nAfter"), ("review.acceptAll", "Heading\nNew one\nNew two\nAfter")] {
            let mut s = Session::new(Document::from_text("Heading\nOld one\nOld two\nAfter"));
            s.run("review.trackChanges", &json!({"value": true})).unwrap();
            replace_blocks(&mut s, StoryRef::Body, 1, 3, new()).unwrap();
            s.run(answer, &json!({})).unwrap();
            assert_eq!(body(&s), want, "{answer}");
        }

        let mut s = Session::new(Document::from_text("Heading\nOld one\nOld two\nAfter"));
        replace_blocks(&mut s, StoryRef::Body, 1, 3, new()).unwrap();
        assert_eq!(body(&s), "Heading\nNew one\nNew two\nAfter");
        assert!(s.doc.revisions.is_empty());
    }
}
