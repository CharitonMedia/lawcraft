//! The paper's title in the footer, below the page number and divided from the page by a
//! printed line (California Rules of Court 2.110): a `TITLE` field, so it shows File ›
//! Properties › Title and follows it. `law.pleadings.new` sets that property from the
//! pleading's title.
//!
//! Typed through WordCraft's commands like everything else in LawCraft, joined into the calling
//! command's undo step. A footer that already shows a `TITLE` field is left as it is, so applying
//! a profile twice doesn't add a second title.

use serde_json::{Value, json};
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Document, Path, Pos, StoryRef};
use wordcraft_engine::{CmdError, CmdResult, Selection, Session};

use crate::profile::FooterTitleSpec;

fn run(s: &mut Session, id: &str, params: Value) -> CmdResult {
    s.join_next_undo();
    s.run(id, &params)
}

/// Whether footer `id` already holds a `TITLE` field.
pub fn has_title(doc: &Document, id: u32) -> bool {
    let story = StoryRef::Part(id);
    doc.para_paths(story).iter().any(|path| {
        doc.para(story, path).is_some_and(|p| {
            p.object_offsets().into_iter().any(
                |off| matches!(p.object_at(off), Some(InlineObject::Field { instr, .. }) if instr.trim().to_ascii_uppercase().starts_with("TITLE")),
            )
        })
    })
}

/// Add the title line to the default footer of the first section (made if it has none).
pub fn apply(s: &mut Session, ft: &FooterTitleSpec) -> Result<(), CmdError> {
    let caret = s.sel.clone();
    s.sel = Selection::caret(Pos { story: StoryRef::Body, path: Path::top(0), off: 0 });
    if let Some(d) = ft.distance {
        run(s, "hf.position", json!({"footer": d}))?;
    }
    let r = run(s, "insert.createFooter", json!({}))?;
    let Some(id) = r.get("story").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()) else {
        s.sel = caret;
        return Ok(());
    };
    if !has_title(&s.doc, id) {
        let story = StoryRef::Part(id);
        let empty = s.doc.plain_text(story).trim().is_empty() && s.doc.para_paths(story).len() <= 1 && !footer_has_objects(&s.doc, id);
        s.sel = Selection::caret(s.doc.end_of(story));
        if !empty {
            run(s, "text.insert", json!({"text": "\r", "raw": true}))?;
            run(s, "para.reset", json!({}))?;
        }
        run(s, "para.alignCenter", json!({}))?;
        if ft.rule {
            run(s, "para.borders", json!({"sides": {"top": {"style": "single", "width": 0.75, "color": "000000"}}}))?;
        }
        run(s, "insert.field", json!({"instr": "TITLE"}))?;
        run(s, "select.paragraph", json!({}))?;
        run(s, "format.size", json!({"size": ft.size}))?;
    }
    s.sel = caret;
    Ok(())
}

fn footer_has_objects(doc: &Document, id: u32) -> bool {
    let story = StoryRef::Part(id);
    doc.para_paths(story).iter().any(|path| doc.para(story, path).is_some_and(|p| !p.object_offsets().is_empty()))
}
