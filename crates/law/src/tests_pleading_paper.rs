//! Pleading paper (`law.pleadingPaper.*`), the footer title and the California superior court
//! profile: what goes in the headers, that it replaces rather than stacks, comes off cleanly,
//! survives a .docx round trip and shows on every page.

use serde_json::{Value, json};
use wordcraft_doc::para::{InlineObject, ShapeKind};
use wordcraft_doc::{Document, PartKind, StoryRef};
use wordcraft_engine::Session;

use super::tests::{err, law, run};
use crate::pleading_paper::{MARKER, get};

/// LawCraft's objects in the document's headers: (header id, object).
fn marked(doc: &Document) -> Vec<(u32, InlineObject)> {
    let mut v = Vec::new();
    for (id, part) in &doc.parts {
        if part.kind != PartKind::Header {
            continue;
        }
        let story = StoryRef::Part(*id);
        for path in doc.para_paths(story) {
            let Some(p) = doc.para(story, &path) else { continue };
            for off in p.object_offsets() {
                if let Some(o) = p.object_at(off).filter(|o| o.frame().is_some_and(|(_, _, f)| f.alt.title == MARKER)) {
                    v.push((*id, o.clone()));
                }
            }
        }
    }
    v
}

/// The numbers in the number box(es), one list per box.
fn numbers(doc: &Document) -> Vec<Vec<String>> {
    marked(doc)
        .into_iter()
        .filter_map(|(_, o)| match o {
            InlineObject::Shape { kind: ShapeKind::TextBox, story: Some(id), .. } => {
                let story = StoryRef::Part(id);
                Some(doc.plain_text(story).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
            }
            _ => None,
        })
        .collect()
}

fn connectors(doc: &Document) -> usize {
    marked(doc).iter().filter(|(_, o)| matches!(o, InlineObject::Shape { kind: ShapeKind::StraightConnector, .. })).count()
}

fn with_text(s: &mut Session, words: usize) {
    let text = "Plaintiff alleges as follows. ".repeat(words / 4 + 1);
    run(s, "text.insert", json!({"text": text}));
}

fn one_to(n: u32) -> Vec<String> {
    (1..=n).map(|i| i.to_string()).collect()
}

#[test]
fn apply_draws_one_number_box_and_three_rules_in_the_header() {
    let mut s = law();
    let r = run(&mut s, "law.pleadingPaper.apply", json!({}));
    assert_eq!(r["lines"], 28);
    assert_eq!(numbers(&s.doc), vec![one_to(28)]);
    assert_eq!(connectors(&s.doc), 3, "a double rule and a single one");
    let headers: std::collections::BTreeSet<u32> = marked(&s.doc).into_iter().map(|(h, _)| h).collect();
    assert_eq!(headers.len(), 1);
    for (_, o) in marked(&s.doc) {
        let (_, _, f) = o.frame().expect("a floating object");
        assert_eq!(f.wrap, wordcraft_doc::para::Wrap::BehindText);
        assert!(f.alt.decorative, "screen readers skip it");
    }
}

#[test]
fn applying_again_replaces_the_set() {
    let mut s = law();
    run(&mut s, "law.pleadingPaper.apply", json!({}));
    run(&mut s, "law.pleadingPaper.apply", json!({}));
    assert_eq!(numbers(&s.doc).len(), 1);
    assert_eq!(connectors(&s.doc), 3);
    run(&mut s, "law.pleadingPaper.apply", json!({"lines": 25, "leftRule": "single", "rightRule": "none"}));
    assert_eq!(numbers(&s.doc), vec![one_to(25)]);
    assert_eq!(connectors(&s.doc), 1);
}

#[test]
fn remove_takes_only_lawcrafts_objects() {
    let mut s = law();
    run(&mut s, "insert.createHeader", json!({}));
    run(&mut s, "text.insert", json!({"text": "Attorney for Plaintiff"}));
    run(&mut s, "law.pleadingPaper.apply", json!({}));
    let r = run(&mut s, "law.pleadingPaper.remove", json!({}));
    assert_eq!(r["removed"], 4);
    assert!(marked(&s.doc).is_empty());
    let header = s.doc.parts.iter().find(|(_, p)| p.kind == PartKind::Header).map(|(id, _)| *id).expect("header");
    assert!(s.doc.plain_text(StoryRef::Part(header)).contains("Attorney for Plaintiff"), "the person's header text stays");
    assert_eq!(run(&mut s, "law.pleadingPaper.remove", json!({}))["removed"], 0);
}

#[test]
fn toggle_turns_it_on_and_off() {
    let mut s = law();
    assert_eq!(run(&mut s, "law.pleadingPaper.toggle", json!({}))["on"], true);
    assert!(get(&s.doc).is_some());
    assert_eq!(run(&mut s, "law.pleadingPaper.toggle", json!({}))["on"], false);
    assert!(get(&s.doc).is_none());
}

#[test]
fn one_undo_takes_it_away() {
    let mut s = law();
    with_text(&mut s, 20);
    let before = s.doc.clone();
    let steps = s.undo_depth();
    run(&mut s, "law.pleadingPaper.apply", json!({}));
    assert_eq!(s.undo_depth(), steps + 1, "one step");
    s.run("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.doc, before);
}

#[test]
fn get_reads_the_settings_back() {
    let mut s = law();
    assert_eq!(run(&mut s, "law.pleadingPaper.get", json!({})), Value::Null);
    let want = json!({"lines": 26, "spacing": 24.0, "start": 2.0, "numberFont": "Arial", "numberSize": 11.0, "numberGap": 6.0, "leftRule": "single", "rightRule": "none", "ruleGap": 5.0, "ruleWidth": 1.0});
    run(&mut s, "law.pleadingPaper.apply", want.clone());
    let got = run(&mut s, "law.pleadingPaper.get", json!({}));
    for (k, v) in want.as_object().expect("object") {
        assert_eq!(&got[k], v, "{k}");
    }
    assert_eq!(got["enabled"], true);
}

#[test]
fn bad_settings_are_refused() {
    let mut s = law();
    for bad in [json!({"lines": 5}), json!({"spacing": 40}), json!({"leftRule": "triple"}), json!({"rightRule": "double"}), json!({"color": "red"})] {
        err(&mut s, "law.pleadingPaper.apply", bad);
    }
    assert!(marked(&s.doc).is_empty());
}

#[test]
fn a_first_page_header_gets_a_set_too() {
    let mut s = law();
    with_text(&mut s, 2000);
    assert!(s.layout().pages.len() >= 2);
    run(&mut s, "layout.differentFirstPage", json!({"value": true}));
    run(&mut s, "law.pleadingPaper.apply", json!({}));
    let headers: std::collections::BTreeSet<u32> = marked(&s.doc).into_iter().map(|(h, _)| h).collect();
    assert_eq!(headers.len(), 2, "the first-page header and the default one");
    assert_eq!(numbers(&s.doc).len(), 2);
}

#[test]
fn a_docx_round_trip_keeps_it() {
    let mut s = law();
    with_text(&mut s, 400);
    run(&mut s, "law.pleadingPaper.apply", json!({"lines": 27}));
    let bytes = wordcraft_engine::io::save_bytes("pleading.docx", &s.doc).expect("save");
    let reopened = wordcraft_engine::io::open_bytes("pleading.docx", &bytes).expect("reopen");
    assert_eq!(numbers(&reopened), vec![one_to(27)]);
    assert_eq!(connectors(&reopened), 3);
    assert_eq!(get(&reopened).map(|p| p.lines), Some(27));
    for (_, o) in marked(&reopened) {
        if matches!(o, InlineObject::Shape { kind: ShapeKind::StraightConnector, .. }) {
            let (_, _, f) = o.frame().expect("frame");
            assert!((f.rot - 90.0).abs() < 0.01, "rotated upright: {}", f.rot);
        }
    }
}

#[test]
fn profiles_validate_pleading_paper() {
    let mut s = law();
    let mut p = run(&mut s, "law.profiles.get", json!({"id": "california-superior-court"}));
    p["id"] = json!("too-many-lines");
    p["verified"] = json!(false);
    p["docs"]["pleading"]["pleadingPaper"]["lines"] = json!(40);
    let e = err(&mut s, "law.profiles.validate", p.clone());
    assert!(e.to_string().contains("pleadingPaper"), "{e}");
    p["docs"]["pleading"]["pleadingPaper"]["lines"] = json!(28);
    p["docs"]["pleading"]["lineNumbers"] = json!({"enabled": true});
    let e = err(&mut s, "law.profiles.validate", p);
    assert!(e.to_string().contains("can't both be on"), "{e}");
}

#[test]
fn the_california_profile_sets_up_a_pleading() {
    let mut s = law();
    run(&mut s, "file.properties", json!({"title": "Complaint for Damages"}));
    let r = run(&mut s, "law.profiles.apply", json!({"id": "california-superior-court", "doc": "pleading"}));
    let applied: Vec<String> = r["applied"].as_array().expect("applied").iter().filter_map(|v| v.as_str().map(String::from)).collect();
    assert!(applied.contains(&"law.pleadingPaper.apply".to_string()), "{applied:?}");
    assert!(applied.contains(&"law.footerTitle".to_string()), "{applied:?}");
    assert_eq!(numbers(&s.doc), vec![one_to(28)]);
    let (_, sect) = s.doc.sections()[0];
    assert_eq!((sect.margin_left, sect.margin_right, sect.margin_top, sect.margin_bottom), (90.0, 36.0, 72.0, 48.0));
    let footer = sect.footers.default.expect("a footer");
    assert!(crate::footer_title::has_title(&s.doc, footer), "the TITLE field");
    // Twice: still one set of numbers.
    run(&mut s, "law.profiles.apply", json!({"id": "california-superior-court", "doc": "pleading"}));
    assert_eq!(numbers(&s.doc).len(), 1);
    let check = run(&mut s, "law.checks.run", json!({}));
    assert!(!check.to_string().contains("Pleading paper is required"), "{check}");
}

#[test]
fn missouri_profiles_draw_no_pleading_paper() {
    let mut s = law();
    run(&mut s, "law.profiles.apply", json!({"id": "missouri-circuit-court", "doc": "motion"}));
    assert!(marked(&s.doc).is_empty());
}

#[test]
fn the_pleading_paper_check_flags_a_document_without_it() {
    let mut s = law();
    run(&mut s, "law.profiles.apply", json!({"id": "california-superior-court", "doc": "pleading"}));
    run(&mut s, "law.pleadingPaper.remove", json!({}));
    let check = run(&mut s, "law.checks.run", json!({}));
    assert!(check.to_string().contains("Pleading paper is required"), "{check}");
}

/// Every page of a three-page California pleading shows the box and the three rules.
#[test]
fn every_page_shows_the_numbers_and_rules() {
    let mut s = law();
    run(&mut s, "law.profiles.apply", json!({"id": "california-superior-court", "doc": "pleading"}));
    with_text(&mut s, 1600);
    let layout = s.layout();
    assert!(layout.pages.len() >= 3, "{} pages", layout.pages.len());
    let all = marked(&s.doc);
    for (i, pg) in layout.pages.iter().enumerate() {
        let id = pg.header_story.expect("every page shows a header");
        assert_eq!(all.iter().filter(|(h, _)| *h == id).count(), 4, "page {i}: the box and three rules");
    }
    let pdf = wordcraft_engine::io::save_bytes_opts("pleading.pdf", &s.doc, &Default::default()).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
}
