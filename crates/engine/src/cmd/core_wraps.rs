//! Core command wraps that LawCraft's fork always installs (prefix, styles, TOA, cell margins).
//! Keeps WordCraft command files byte-identical to upstream.

use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Block, StoryRef};

use crate::{CmdError, CmdResult, Session, p};

static PAGE_NUMBER: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static PAGE_NUMBER_FORMAT: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static STYLES_CREATE: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static STYLES_MODIFY: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static MARK_CITATION: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static TABLE_OF_AUTHORITIES: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static CELL_MARGINS: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static REVIEW_ISSUES: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static REVIEW_SPELLING: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static REVIEW_SUGGESTIONS: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static CUSTOMIZE_RIBBON: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Merge document lint findings into `review.issues` (add-in platform), with this session's
/// proofing module scoped for the core call.
fn wrap_review_issues(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = REVIEW_ISSUES.get() else {
        return Err(CmdError::Failed("review.issues wrap missing".into()));
    };
    let _guard = wordcraft_proof::enter_session_module(s.module_en);
    let core = orig(s, v)?;
    let mut out = s.lint_findings();
    if let Some(arr) = core.as_array() {
        out.extend(arr.iter().cloned());
    }
    if out.len() > 500 {
        out.truncate(500);
    }
    Ok(Value::Array(out))
}

fn wrap_review_spelling(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = REVIEW_SPELLING.get() else {
        return Err(CmdError::Failed("review.spelling wrap missing".into()));
    };
    let _guard = wordcraft_proof::enter_session_module(s.module_en);
    orig(s, v)
}

fn wrap_review_suggestions(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = REVIEW_SUGGESTIONS.get() else {
        return Err(CmdError::Failed("review.suggestions wrap missing".into()));
    };
    let _guard = wordcraft_proof::enter_session_module(s.module_en);
    orig(s, v)
}

/// Keep installed add-in tabs across Customize the Ribbon reset/load.
fn wrap_customize_ribbon(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = CUSTOMIZE_RIBBON.get() else {
        return Err(CmdError::Failed("tools.customizeRibbon wrap missing".into()));
    };
    let mut v2 = v.clone();
    let mut handled = false;
    if let Some(x) = v.get("layout") {
        s.ribbon.replace_layout(crate::RibbonLayout::from_value(x));
        let reg = Arc::clone(&s.registry);
        s.ribbon.retain_known(&reg);
        if let Some(obj) = v2.as_object_mut() {
            obj.remove("layout");
        }
        handled = true;
    }
    if v.get("reset").and_then(Value::as_bool) == Some(true) {
        s.ribbon.reset_ribbon();
        s.ribbon.reset_qat();
        if let Some(obj) = v2.as_object_mut() {
            obj.remove("reset");
        }
        handled = true;
    }
    let remaining = v2.as_object().is_some_and(|o| !o.is_empty());
    if remaining {
        return orig(s, &v2);
    }
    if handled {
        return Ok(json!({
            "tabs": serde_json::to_value(s.ribbon.tabs().as_ref()).unwrap_or(Value::Null),
            "qat": s.ribbon.qat(),
        }));
    }
    orig(s, v)
}

fn read_prefix(v: &Value) -> Result<Option<&str>, CmdError> {
    let Some(prefix) = p::str(v, "prefix") else {
        return Ok(None);
    };
    if prefix.chars().count() > 16 {
        return Err(CmdError::Params("`prefix` is at most 16 characters".into()));
    }
    Ok(Some(prefix))
}

fn set_section_prefix(s: &mut Session, pfx: &str) -> Result<(), CmdError> {
    let i = super::page::section_index_of_focus(s);
    let end = s.doc.sections().get(i).map(|(e, _)| *e).unwrap_or(0);
    s.doc.section_mut(end).page_num_prefix = if pfx.is_empty() { None } else { Some(pfx.to_string()) };
    Ok(())
}

fn page_field_story(s: &Session, v: &Value) -> Option<StoryRef> {
    let position = p::str(v, "position").unwrap_or("bottom");
    if position == "current" {
        return None;
    }
    let header = position == "top";
    let sect = super::page::sect(s);
    let id = if header { sect.headers.default } else { sect.footers.default }?;
    Some(StoryRef::Part(id))
}

fn apply_prefix_to_gallery(s: &mut Session, v: &Value, pfx: &str) -> Result<(), CmdError> {
    let Some(story) = page_field_story(s, v) else {
        return Ok(());
    };
    let x_of_y = p::str(v, "format").is_some_and(|f| f.contains("of"));
    let blocks = s.doc.story_mut(story)?;
    let Some(block) = blocks.iter_mut().rev().find(|b| {
        b.as_para()
            .is_some_and(|p| p.objects.iter().any(|o| matches!(o, InlineObject::Field { instr, .. } if instr.trim_start().starts_with("PAGE"))))
    }) else {
        return Ok(());
    };
    let Block::Para(para) = Arc::make_mut(block) else {
        return Ok(());
    };
    if x_of_y {
        if para.plain_text().starts_with("Page ") {
            para.delete(0, 5)?;
        }
        if !pfx.is_empty() {
            para.insert_text(0, pfx, &CharProps::default())?;
        }
    } else if !pfx.is_empty() {
        para.insert_text(0, pfx, &CharProps::default())?;
    }
    Ok(())
}

fn wrap_page_number(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = PAGE_NUMBER.get() else {
        return Err(CmdError::Failed("insert.pageNumber wrap missing".into()));
    };
    let prefix_param = read_prefix(v)?;
    let mut v2 = v.clone();
    if let Some(obj) = v2.as_object_mut() {
        obj.remove("prefix");
    }
    let out = orig(s, &v2)?;
    if let Some(pfx) = prefix_param {
        set_section_prefix(s, pfx)?;
        if !pfx.is_empty() {
            apply_prefix_to_gallery(s, v, pfx)?;
        }
    }
    Ok(out)
}

fn wrap_page_number_format(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = PAGE_NUMBER_FORMAT.get() else {
        return Err(CmdError::Failed("layout.pageNumberFormat wrap missing".into()));
    };
    let prefix_param = read_prefix(v)?;
    let mut v2 = v.clone();
    if let Some(obj) = v2.as_object_mut() {
        obj.remove("prefix");
    }
    let out = orig(s, &v2)?;
    if let Some(pfx) = prefix_param {
        set_section_prefix(s, pfx)?;
    }
    Ok(out)
}

fn wrap_styles_create(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = STYLES_CREATE.get() else {
        return Err(CmdError::Failed("styles.create wrap missing".into()));
    };
    let apply = p::bool(v, "apply").unwrap_or(true);
    if apply {
        return orig(s, v);
    }
    let prev = s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone());
    let out = orig(s, v)?;
    let focus = s.sel.focus.clone();
    if let Ok(para) = s.doc.para_mut(focus.story, &focus.path) {
        para.props.style = prev;
    }
    Ok(out)
}

fn wrap_styles_modify(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = STYLES_MODIFY.get() else {
        return Err(CmdError::Failed("styles.modify wrap missing".into()));
    };
    let based_on = p::str(v, "basedOn").map(str::to_string);
    let style_name = p::str(v, "style").map(str::to_string);
    let out = orig(s, v)?;
    if let (Some(b), Some(name)) = (based_on, style_name) {
        let id = s.doc.styles.find(&name).map(|x| x.id.clone()).unwrap_or(name);
        let based = s.doc.styles.find(&b).map(|x| x.id.clone()).unwrap_or(b);
        if let Some(st) = Arc::make_mut(&mut s.doc.styles).get_mut(&id) {
            st.based_on = Some(based);
        }
    }
    Ok(out)
}

fn ta_field(entry: &str, category: Option<u8>) -> InlineObject {
    let entry = entry.replace('"', "");
    let instr = match category {
        Some(c) => format!("TA \\l \"{entry}\" \\c {c}"),
        None => format!("TA \\l \"{entry}\""),
    };
    InlineObject::Field { instr, result: String::new(), locked: false, dirty: false, code: false, form: String::new() }
}

fn wrap_mark_citation(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = MARK_CITATION.get() else {
        return Err(CmdError::Failed("references.markCitation wrap missing".into()));
    };
    let category = match p::u64(v, "category") {
        Some(c) if (1..=16).contains(&c) => Some(c as u8),
        Some(_) => return Err(CmdError::Params("`category` is 1-16".into())),
        None => None,
    };
    if category.is_none() {
        return orig(s, v);
    }
    let entry = p::str(v, "entry").map(str::to_string).unwrap_or_else(|| s.selected_text().trim().to_string());
    if entry.is_empty() {
        return Err(CmdError::Params("select a citation or give `entry`".into()));
    }
    let (_, b) = s.sel.ordered();
    s.doc.insert_object(&b, ta_field(&entry, category), &CharProps { hidden: Some(true), ..Default::default() })?;
    Ok(json!({"entry": entry}))
}

fn wrap_table_of_authorities(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = TABLE_OF_AUTHORITIES.get() else {
        return Err(CmdError::Failed("references.tableOfAuthorities wrap missing".into()));
    };
    let has_extra = v.get("category").is_some() || v.get("passim").and_then(Value::as_bool) == Some(true) || v.get("title").is_some();
    if !has_extra {
        return orig(s, v);
    }
    let mut instr = String::from("TOA");
    match p::u64(v, "category") {
        Some(c) if (1..=16).contains(&c) => instr.push_str(&format!(" \\c \"{c}\"")),
        Some(_) => return Err(CmdError::Params("`category` is 1-16".into())),
        None => {}
    }
    if p::bool(v, "passim").unwrap_or(false) {
        instr.push_str(" \\p");
    }
    let title: String = p::str(v, "title").unwrap_or("Table of Authorities").chars().filter(|c| !c.is_control()).take(255).collect();
    s.insert_generated_list(&instr, &title)?;
    super::sel_result(s)
}

fn wrap_cell_margins(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = CELL_MARGINS.get() else {
        return Err(CmdError::Failed("table.cellMargins wrap missing".into()));
    };
    if !p::bool(v, "cell").unwrap_or(false) {
        return orig(s, v);
    }
    let Some((tp, row, col)) = s.sel.focus.path.cell() else {
        return Err(CmdError::Disabled("not in a table".into()));
    };
    let story = s.sel.focus.story;
    let current = {
        let table = s.doc.table(story, &tp).ok_or_else(|| CmdError::Failed("no table".into()))?;
        let style = table.props.style.as_deref().and_then(|id| s.doc.styles.table_style(id));
        let whole = table.props.cell_margins.or(style.and_then(|st| st.parts.cell_margins)).unwrap_or([0.0, 5.4, 0.0, 5.4]);
        table.rows.get(row).and_then(|r| r.cells.get(col)).and_then(|c| c.props.margins).unwrap_or(whole)
    };
    let m = [
        p::f32(v, "top").unwrap_or(current[0]),
        p::f32(v, "left").unwrap_or(current[1]),
        p::f32(v, "bottom").unwrap_or(current[2]),
        p::f32(v, "right").unwrap_or(current[3]),
    ]
    .map(|x| x.clamp(0.0, 144.0));
    let t = s.doc.table_mut(story, &tp)?;
    let cell = t.rows.get_mut(row).and_then(|r| r.cells.get_mut(col)).ok_or_else(|| CmdError::Disabled("not in a table".into()))?;
    cell.props.margins = Some(m);
    for path in s.doc.para_paths(story).into_iter().filter(|p| p.0.starts_with(&tp.0) && p.0.len() > tp.0.len()) {
        s.doc.para_mut(story, &path)?.touch();
    }
    super::sel_result(s)
}

fn capture_and_replace(
    session: &mut Session,
    id: &str,
    slot: &OnceLock<fn(&mut Session, &Value) -> CmdResult>,
    run: fn(&mut Session, &Value) -> CmdResult,
) {
    session.replace_command(id, |mut spec| {
        let _ = slot.set(spec.run);
        spec.run = run;
        spec
    });
}

fn enrich_param_docs(session: &mut Session) {
    session.replace_command("insert.pageNumber", |mut spec| {
        spec.params = concat!(
            r#"{"position"?: "top|bottom|current", "align"?: "left|center|right", "format"?: "x of y","#,
            r#" "prefix"?: string (text before the page number, e.g. appendix "A")}"#
        );
        spec
    });
    session.replace_command("layout.pageNumberFormat", |mut spec| {
        spec.params = concat!(
            r#"{"format"?: "decimal|lowerRoman|upperRoman|lowerLetter|upperLetter", "start"?: n | null,"#,
            r#" "prefix"?: string (text before the page number, e.g. appendix "A")}"#,
            r#" ("start" left out: the numbering keeps its start; null: the pages number on from the section before; a number: numbering starts there)"#
        );
        spec
    });
    session.replace_command("styles.create", |mut spec| {
        spec.params = concat!(
            r#"{"name": string, "basedOn"?: string, "fromSelection"?: bool (default: true),"#,
            r#" "apply"?: bool (apply the new style to the selection; default: true)}"#
        );
        spec
    });
    session.replace_command("styles.modify", |mut spec| {
        spec.params = r#"{"style": string, "chr"?: CharProps, "para"?: ParaProps, "name"?: string, "basedOn"?: string, "next"?: string}"#;
        spec
    });
    session.replace_command("references.markCitation", |mut spec| {
        spec.params = r#"{"entry"?: string, "category"?: 1-16 (Word's categories: 1 Cases, 2 Statutes, 3 Other Authorities, 4 Rules, 5 Treatises, 6 Regulations, 7 Constitutional Provisions)}"#;
        spec
    });
    session.replace_command("references.tableOfAuthorities", |mut spec| {
        spec.params =
            r#"{"category"?: 1-16 (TOA \c: only citations marked with that category), "passim"?: bool (TOA \p: "passim" for an authority cited on five or more pages), "title"?: string}"#;
        spec
    });
    session.replace_command("table.cellMargins", |mut spec| {
        spec.params = r#"{"top"?, "left"?, "bottom"?, "right"? (pt), "cell"?: bool (true: the caret cell only, not the whole table)}"#;
        spec
    });
}

/// Install wraps once per process (each new session's registry is rebuilt from the wrapped one).
pub fn install(session: &mut Session) {
    if INSTALLED.get().is_some() {
        // A fresh session starts from `cmd::registry()` again; re-apply wraps to this registry.
        reapply(session);
        return;
    }
    let _ = INSTALLED.set(());
    reapply(session);
}

fn reapply(session: &mut Session) {
    // OnceLocks keep the original run fns from the first install; later sessions only swap run pointers.
    if PAGE_NUMBER.get().is_none() {
        capture_and_replace(session, "insert.pageNumber", &PAGE_NUMBER, wrap_page_number);
        capture_and_replace(session, "layout.pageNumberFormat", &PAGE_NUMBER_FORMAT, wrap_page_number_format);
        capture_and_replace(session, "styles.create", &STYLES_CREATE, wrap_styles_create);
        capture_and_replace(session, "styles.modify", &STYLES_MODIFY, wrap_styles_modify);
        capture_and_replace(session, "references.markCitation", &MARK_CITATION, wrap_mark_citation);
        capture_and_replace(session, "references.tableOfAuthorities", &TABLE_OF_AUTHORITIES, wrap_table_of_authorities);
        capture_and_replace(session, "table.cellMargins", &CELL_MARGINS, wrap_cell_margins);
        capture_and_replace(session, "review.issues", &REVIEW_ISSUES, wrap_review_issues);
        capture_and_replace(session, "review.spelling", &REVIEW_SPELLING, wrap_review_spelling);
        capture_and_replace(session, "review.suggestions", &REVIEW_SUGGESTIONS, wrap_review_suggestions);
        capture_and_replace(session, "tools.customizeRibbon", &CUSTOMIZE_RIBBON, wrap_customize_ribbon);
    } else {
        for (id, run) in [
            ("insert.pageNumber", wrap_page_number as fn(&mut Session, &Value) -> CmdResult),
            ("layout.pageNumberFormat", wrap_page_number_format),
            ("styles.create", wrap_styles_create),
            ("styles.modify", wrap_styles_modify),
            ("references.markCitation", wrap_mark_citation),
            ("references.tableOfAuthorities", wrap_table_of_authorities),
            ("table.cellMargins", wrap_cell_margins),
            ("review.issues", wrap_review_issues),
            ("review.spelling", wrap_review_spelling),
            ("review.suggestions", wrap_review_suggestions),
            ("tools.customizeRibbon", wrap_customize_ribbon),
        ] {
            session.replace_command(id, |mut spec| {
                spec.run = run;
                spec
            });
        }
    }
    enrich_param_docs(session);
}
