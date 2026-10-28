//! Pleading paper: consecutive line numbers down the left margin of every page (1 to 28 by
//! default), a double rule between the numbers and the text and a single rule on the right, as
//! California Rules of Court 2.108 asks for and Word's pleading templates draw.
//!
//! Like the rest of LawCraft, nothing here edits the document tree: the numbers are a text box
//! and the rules are straight connectors, put in each page header through WordCraft's own
//! commands (`insert.textBox`, `insert.shape`, `arrange.*`, `object.altText`), behind the text
//! and fixed to the page, all joined into the calling command's one undo step. Every page that
//! shows the header shows them, and a `.docx` keeps them as ordinary header drawings that Word
//! and LibreOffice draw too.
//!
//! Every object carries [`MARKER`] as its alt-text title, so applying again replaces the set
//! instead of adding one, removing takes exactly LawCraft's objects, and the number box's alt
//! text holds the settings, so [`get`] reads them back from the document itself.

use serde_json::{Value, json};
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::{Document, PartKind, Path, Pos, StoryRef};
use wordcraft_engine::cmd::pos_json;
use wordcraft_engine::{CmdError, CmdResult, Selection, Session};

use crate::profile::{PleadingPaperSpec, check_pleading_paper};

/// The alt-text title every pleading-paper object carries.
pub const MARKER: &str = "LawCraft pleading paper";

/// Width of the number box, points.
const BOX_W: f32 = 40.0;
/// Word's default text-box insets, points: the box's text starts this far in.
const INSET_TOP: f32 = 3.6;
const INSET_RIGHT: f32 = 7.2;
/// Space between the two lines of a double rule, points.
const DOUBLE_GAP: f32 = 2.5;
/// How far the rules stop short of the page's top and bottom edges, points.
const RULE_EDGE: f32 = 24.0;

/// `law.pleadingPaper.apply`: pleading paper on every page, with `v`'s settings over the
/// defaults.
pub fn apply_cmd(s: &mut Session, v: &Value) -> CmdResult {
    let spec = spec_from(v)?;
    apply(s, &spec)?;
    Ok(spec_value(&spec))
}

/// `law.pleadingPaper.remove`: take LawCraft's pleading-paper objects out of every header.
pub fn remove_cmd(s: &mut Session, _: &Value) -> CmdResult {
    let n = remove(s)?;
    Ok(json!({"removed": n}))
}

/// `law.pleadingPaper.get`: the settings in effect, or `null` when the document has none.
pub fn get_cmd(s: &mut Session, _: &Value) -> CmdResult {
    Ok(get(&s.doc).map_or(Value::Null, |spec| spec_value(&spec)))
}

/// `law.pleadingPaper.toggle`: on with the defaults when the document has none, off when it has.
pub fn toggle_cmd(s: &mut Session, v: &Value) -> CmdResult {
    if get(&s.doc).is_some() {
        let n = remove(s)?;
        return Ok(json!({"on": false, "removed": n}));
    }
    let spec = spec_from(v)?;
    apply(s, &spec)?;
    Ok(json!({"on": true, "settings": spec_value(&spec)}))
}

/// The settings `v` gives, over the defaults, checked.
fn spec_from(v: &Value) -> Result<PleadingPaperSpec, CmdError> {
    let v = if v.is_null() { json!({}) } else { v.clone() };
    let spec: PleadingPaperSpec = serde_json::from_value(v).map_err(|e| CmdError::Params(e.to_string()))?;
    let mut problems = Vec::new();
    check_pleading_paper(&mut problems, "pleadingPaper", &spec);
    if problems.is_empty() { Ok(spec) } else { Err(CmdError::Params(problems.join("; "))) }
}

/// The settings as the commands return them (every field, defaults filled in).
pub fn spec_value(spec: &PleadingPaperSpec) -> Value {
    serde_json::to_value(spec).unwrap_or(Value::Null)
}

/// Put pleading paper on every page: first take off any LawCraft set, then draw one in every
/// header a page shows (creating the header where a page has none) and in every first-page and
/// even-page header the sections define. `enabled: false` only takes it off.
pub fn apply(s: &mut Session, spec: &PleadingPaperSpec) -> Result<(), CmdError> {
    remove(s)?;
    if !spec.enabled {
        return Ok(());
    }
    let caret = s.sel.clone();
    for (header, geometry) in headers(s)? {
        draw(s, spec, header, &geometry)?;
    }
    s.sel = caret;
    Ok(())
}

/// Run a command as part of the calling command's undo step.
fn run(s: &mut Session, id: &str, params: Value) -> CmdResult {
    s.join_next_undo();
    s.run(id, &params)
}

/// The page geometry a header's objects are placed against.
#[derive(Clone, Copy)]
struct Geometry {
    page_h: f32,
    top: f32,
    text_w: f32,
}

fn geometry(d: &Document, section: usize) -> Geometry {
    let sections = d.sections();
    let sect = sections.get(section).or_else(|| sections.first()).map(|(_, p)| *p);
    sect.map_or(Geometry { page_h: 792.0, top: 72.0, text_w: 468.0 }, |p| Geometry { page_h: p.page_h, top: p.margin_top, text_w: p.text_width() })
}

/// Every header to draw in, with the geometry of the first section that uses it.
fn headers(s: &mut Session) -> Result<Vec<(u32, Geometry)>, CmdError> {
    let mut out: Vec<(u32, Geometry)> = Vec::new();
    // One header per (section, slot) a page shows; `insert.createHeader` makes the missing ones.
    let layout = s.layout();
    let mut seen = Vec::new();
    let mut pages = Vec::new();
    for (i, pg) in layout.pages.iter().enumerate() {
        let key = (pg.section, format!("{:?}", pg.hf_slot));
        if !seen.contains(&key) {
            seen.push(key);
            pages.push((i, pg.section));
        }
    }
    for (page, section) in pages {
        let r = run(s, "insert.createHeader", json!({"page": page}))?;
        let Some(id) = r.get("story").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()) else { continue };
        if !out.iter().any(|(h, _)| *h == id) {
            out.push((id, geometry(&s.doc, section)));
        }
    }
    // First-page and even-page headers no page shows yet still get a set, so the paper is there
    // when the document grows.
    let defined: Vec<(u32, usize)> = s
        .doc
        .sections()
        .iter()
        .enumerate()
        .flat_map(|(i, (_, p))| [p.headers.default, p.headers.first, p.headers.even].into_iter().flatten().map(move |id| (id, i)))
        .collect();
    for (id, section) in defined {
        if s.doc.parts.contains_key(&id) && !out.iter().any(|(h, _)| *h == id) {
            out.push((id, geometry(&s.doc, section)));
        }
    }
    Ok(out)
}

/// The start of a header's first paragraph, where the objects go.
fn header_start(header: u32) -> Pos {
    Pos { story: StoryRef::Part(header), path: Path::top(0), off: 0 }
}

/// Select the object at the start of `header`.
fn select_first_object(s: &mut Session, header: u32) -> CmdResult {
    s.sel = Selection::caret(header_start(header));
    run(s, "select.addObject", json!({"pos": pos_json(&header_start(header))}))
}

/// Draw one set in `header`: the number box, then the rules.
fn draw(s: &mut Session, spec: &PleadingPaperSpec, header: u32, g: &Geometry) -> Result<(), CmdError> {
    let lines = spec.lines;
    let left_rule_x = -spec.rule_gap;
    let rules_w = if spec.left_rule == "double" { DOUBLE_GAP } else { 0.0 };
    // The number box: its text's right edge `numberGap` left of the outer left rule.
    let box_x = left_rule_x - rules_w - spec.number_gap - (BOX_W - INSET_RIGHT);
    let box_h = lines as f32 * spec.spacing + 2.0 * INSET_TOP + 6.0;
    s.sel = Selection::caret(header_start(header));
    run(s, "insert.textBox", json!({"text": "1", "width": BOX_W, "height": box_h}))?;
    for n in 2..=lines {
        run(s, "text.insert", json!({"text": "\r", "raw": true}))?;
        run(s, "text.insert", json!({"text": n.to_string(), "raw": true}))?;
    }
    run(s, "select.all", json!({}))?;
    run(s, "para.lineSpacing", json!({"exactly": spec.spacing}))?;
    run(s, "para.spacing", json!({"before": 0, "after": 0}))?;
    run(s, "para.alignRight", json!({}))?;
    if let Some(font) = spec.number_font.as_deref().filter(|f| !f.is_empty()) {
        run(s, "format.font", json!({"name": font}))?;
    }
    run(s, "format.size", json!({"size": spec.number_size}))?;
    select_first_object(s, header)?;
    run(s, "shape.outline", json!({"color": null}))?;
    run(s, "shape.fill", json!({"color": null}))?;
    float(s, box_x, spec.start - INSET_TOP)?;
    let settings = serde_json::to_string(spec).unwrap_or_default();
    run(s, "object.altText", json!({"title": MARKER, "text": settings, "decorative": true}))?;

    // The rules: connectors the height of the page, less an edge at the top and bottom.
    let mut xs = Vec::new();
    match spec.left_rule.as_str() {
        "double" => xs.extend([left_rule_x, left_rule_x - DOUBLE_GAP]),
        "single" => xs.push(left_rule_x),
        _ => {}
    }
    if spec.right_rule == "single" {
        xs.push(g.text_w + spec.rule_gap);
    }
    let len = (g.page_h - 2.0 * RULE_EDGE).max(36.0);
    for x in xs {
        s.sel = Selection::caret(header_start(header));
        run(s, "insert.shape", json!({"kind": "straightConnector", "width": len, "height": 0, "stroke": "000000"}))?;
        run(s, "shape.outline", json!({"color": "000000", "width": spec.rule_width}))?;
        run(s, "arrange.rotation", json!({"degrees": 90}))?;
        // Rotated about its centre: the frame's left edge sits half a length left of the line.
        float(s, x - len / 2.0, g.page_h / 2.0 - g.top)?;
        run(s, "object.altText", json!({"title": MARKER, "decorative": true}))?;
    }
    Ok(())
}

/// Behind the text, fixed on the page, at (`x`, `y`) from the top-left of the margins.
fn float(s: &mut Session, x: f32, y: f32) -> Result<(), CmdError> {
    run(s, "arrange.wrap", json!({"wrap": "behindText"}))?;
    run(s, "arrange.moveWithText", json!({"value": false}))?;
    run(s, "arrange.position", json!({"x": x, "y": y}))?;
    Ok(())
}

/// Every LawCraft pleading-paper object in the headers: its position and the object.
fn marked(doc: &Document) -> Vec<(Pos, InlineObject)> {
    let mut v = Vec::new();
    for (id, part) in &doc.parts {
        if part.kind != PartKind::Header {
            continue;
        }
        let story = StoryRef::Part(*id);
        for path in doc.para_paths(story) {
            let Some(p) = doc.para(story, &path) else { continue };
            for off in p.object_offsets() {
                if let Some(o) = p.object_at(off).filter(|o| is_marked(o)) {
                    v.push((Pos { story, path: path.clone(), off }, o.clone()));
                }
            }
        }
    }
    v
}

fn is_marked(o: &InlineObject) -> bool {
    o.frame().is_some_and(|(_, _, f)| f.alt.title == MARKER)
}

/// Take every LawCraft pleading-paper object out; returns how many.
pub fn remove(s: &mut Session) -> Result<usize, CmdError> {
    let found: Vec<Pos> = marked(&s.doc).into_iter().map(|(p, _)| p).collect();
    if found.is_empty() {
        return Ok(0);
    }
    let caret = s.sel.clone();
    // Within a paragraph, last first, so the earlier offsets stay right.
    for pos in found.iter().rev() {
        s.sel = Selection::caret(pos.clone());
        run(s, "select.addObject", json!({"pos": pos_json(pos)}))?;
        run(s, "text.delete", json!({}))?;
    }
    let ok = |p: &Pos| s.doc.para(p.story, &p.path).is_some_and(|para| p.off <= para.len());
    s.sel = if ok(&caret.anchor) && ok(&caret.focus) { caret } else { Selection::caret(Pos { story: StoryRef::Body, path: Path::top(0), off: 0 }) };
    Ok(found.len())
}

/// The settings of the document's pleading paper, read from its number box; `None` when it has
/// none.
pub fn get(doc: &Document) -> Option<PleadingPaperSpec> {
    marked(doc).into_iter().find_map(|(_, o)| serde_json::from_str::<PleadingPaperSpec>(o.alt_text()).ok())
}
