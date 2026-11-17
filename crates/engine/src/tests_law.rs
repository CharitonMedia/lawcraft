//! LawCraft-owned engine tests (kept out of WordCraft's tests.rs).

use serde_json::json;
use wordcraft_doc::{Document, Pos, StoryRef};

use crate::Session;
use crate::cmd::insert::base64_encode;

fn s() -> Session {
    Session::new(Document::new())
}

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn text(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

#[test]
fn a_module_installs_its_commands_and_never_shadows_core() {
    let mut s = s();
    assert!(!s.module_enabled("law"), "a module is off until the user turns it on");
    assert!(s.run("mod.ping", &json!({})).is_err(), "unknown command");
    let spec = crate::CommandSpec::new("mod.ping", "Ping", "Test", |_, _| Ok(json!({"pong": true})));
    assert_eq!(s.install_commands(vec![spec]), 1);
    assert_eq!(run(&mut s, "mod.ping", json!({})), json!({"pong": true}));
    // The core's command wins, and nothing is installed twice.
    let again = crate::CommandSpec::new("mod.ping", "Other", "Test", |_, _| Ok(json!({})));
    assert_eq!(s.install_commands(vec![again]), 0);
    let core = crate::CommandSpec::new("format.bold", "Not Bold", "Test", |_, _| Ok(json!({})));
    assert_eq!(s.install_commands(vec![core]), 0);
    assert_eq!(s.registry.get("format.bold").map(|c| c.label), Some("Bold"));
    // A second session starts from the core registry again.
    let mut fresh = Session::new(Document::new());
    assert!(fresh.run("mod.ping", &json!({})).is_err());
}

#[test]
fn export_checks_see_the_export_options_and_serialise_like_lint() {
    let mut s = s();
    let echo = crate::CommandSpec::new("test.exportEcho", "Echo Export Options", "Test", |_, v| {
        let markup = v.get("markup").and_then(serde_json::Value::as_bool).unwrap_or(false);
        Ok(json!({"findings": [{"kind": "lint", "checkId": "echo", "severity": "error", "message": format!("markup={markup}")}]}))
    });
    let boom = crate::CommandSpec::new("test.exportBoom", "Failing Export Check", "Test", |_, _| Err(crate::CmdError::Failed("boom".into())));
    assert_eq!(s.install_commands(vec![echo, boom]), 2);

    s.set_export_check_commands(vec![("test.exportEcho".into(), json!({"forExport": true}))]);
    // The `markup` override reaches the check: the same stored command sees the current export.
    let marked = crate::io::ExportOptions { markup: true, ..Default::default() };
    let findings = s.run_export_checks(&marked, "pdf");
    assert!(findings.iter().any(|f| f["message"] == "markup=true"), "{findings:?}");
    let plain = crate::io::ExportOptions { markup: false, ..Default::default() };
    let findings = s.run_export_checks(&plain, "pdf");
    assert!(findings.iter().any(|f| f["message"] == "markup=false"), "the option is per export, not baked in: {findings:?}");

    // A check that fails still produces a finding of the same shape as any other lint finding.
    s.set_export_check_commands(vec![("test.exportBoom".into(), json!({}))]);
    let findings = s.run_export_checks(&plain, "pdf");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["kind"], "lint", "{findings:?}");
    assert_eq!(findings[0]["checkId"], "test.exportBoom");
    assert_eq!(findings[0]["severity"], "error");
}

#[test]
fn a_failed_export_check_clears_when_it_runs_again() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FAIL: AtomicBool = AtomicBool::new(true);
    let mut s = s();
    let flaky = crate::CommandSpec::new("test.exportFlaky", "Flaky Export Check", "Test", |_, _| {
        if FAIL.load(Ordering::SeqCst) { Err(crate::CmdError::Failed("offline".into())) } else { Ok(json!({"findings": []})) }
    });
    assert_eq!(s.install_commands(vec![flaky]), 1);
    s.set_export_check_commands(vec![("test.exportFlaky".into(), json!({}))]);
    let plain = crate::io::ExportOptions { markup: false, ..Default::default() };
    s.run_export_checks(&plain, "pdf");
    assert_eq!(s.lint_findings_of("export:test.exportFlaky").len(), 1, "the failure shows in Proofing");
    FAIL.store(false, Ordering::SeqCst);
    assert!(s.run_export_checks(&plain, "pdf").is_empty());
    assert!(s.lint_findings_of("export:test.exportFlaky").is_empty(), "a successful retry drops the old failure");
    assert!(s.lint_findings().is_empty());
}

#[test]
fn export_checks_see_the_output_format() {
    let mut s = s();
    let echo = crate::CommandSpec::new("test.exportFormat", "Echo Format", "Test", |_, v| {
        let format = v.get("format").and_then(serde_json::Value::as_str).unwrap_or("");
        Ok(json!({"findings": [{"kind": "lint", "checkId": "format", "severity": "error", "message": format!("format={format}")}]}))
    });
    assert_eq!(s.install_commands(vec![echo]), 1);
    s.set_export_check_commands(vec![("test.exportFormat".into(), json!({}))]);
    let plain = crate::io::ExportOptions { markup: false, ..Default::default() };
    // A non-PDF export tells the check what it is writing, so PDF-only checks can tell it apart.
    let findings = s.run_export_checks(&plain, "docx");
    assert!(findings.iter().any(|f| f["message"] == "format=docx"), "{findings:?}");
}

#[test]
fn module_settings_round_trip_and_default_off() {
    let mut p = crate::Prefs::default();
    assert!(!p.module_enabled("law"));
    p.modules.insert("law".into(), true);
    let json = serde_json::to_string(&p).expect("prefs serialize");
    assert!(json.contains(r#""modules":{"law":true}"#), "{json}");
    let back: crate::Prefs = serde_json::from_str(&json).expect("prefs deserialize");
    assert!(back.module_enabled("law"));
    // A saved value without the setting (an older file) reads as off, never as on.
    let old: crate::Prefs = serde_json::from_str(r#"{"countNotes": false}"#).expect("older prefs deserialize");
    assert!(!old.module_enabled("law"));
    assert!(!old.count_notes);
}

#[test]
fn styles_create_can_leave_the_selection_alone() {
    let mut s = s();
    run(&mut s, "text.insert", json!({"text": "Hello"}));
    assert_eq!(s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone()), None);
    let r = run(&mut s, "styles.create", json!({"name": "Quotation", "fromSelection": false, "apply": false}));
    let id = r["id"].as_str().expect("style id").to_string();
    assert!(s.doc.styles.get(&id).is_some(), "the style was created");
    assert_eq!(s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone()), None, "`apply: false` leaves the caret's paragraph in its own style");
    // The default is Word's: a new style is applied to the selection.
    run(&mut s, "styles.create", json!({"name": "Other", "fromSelection": false}));
    assert_eq!(s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone()).as_deref(), Some("Other"));
}

#[test]
fn toc_page_numbers_use_the_section_page_prefix() {
    let mut s = s();
    run(&mut s, "insert.pageNumber", json!({"position": "bottom", "prefix": "A"}));
    run(&mut s, "layout.pageNumberFormat", json!({"start": 1}));
    run(&mut s, "references.toc", json!({}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "text.pageBreak", json!({}));
    run(&mut s, "text.insert", json!({"text": "Exhibit"}));
    run(&mut s, "para.heading1", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "references.updateToc", json!({}));
    let t = text(&s);
    assert!(t.contains("Exhibit\tA2"), "TOC page numbers include the section prefix: {t}");
}

#[test]
fn modify_style_can_change_based_on() {
    let mut s = s();
    run(&mut s, "styles.create", json!({"name": "Mine", "basedOn": "Normal", "fromSelection": false, "apply": false}));
    run(&mut s, "styles.create", json!({"name": "Base", "basedOn": "Normal", "fromSelection": false, "apply": false}));
    assert_eq!(s.doc.styles.find("Mine").unwrap().based_on.as_deref(), Some("Normal"));
    run(&mut s, "styles.modify", json!({"style": "Mine", "basedOn": "Base"}));
    assert_eq!(s.doc.styles.find("Mine").unwrap().based_on.as_deref(), Some("Base"));
}

#[test]
fn pdf_import_is_editable_and_save_as_does_not_overwrite_the_source() {
    let original = Document::from_text("Editable PDF text");
    let bytes = wordcraft_pdf::export(&original, &Default::default()).unwrap();
    let dir = std::env::temp_dir().join(format!("wordcraft-pdf-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.pdf");
    std::fs::write(&source, &bytes).unwrap();
    let mut session = s();
    let result = run(&mut session, "file.open", json!({"path": source.to_string_lossy()}));
    assert_eq!(result["import"]["format"], "pdf");
    assert_eq!(result["import"]["saveAsRequired"], true);
    assert!(session.path.is_none() && session.dirty);
    assert_eq!(session.file_name.as_deref(), Some("source"), "Save As suggests the PDF's name");
    assert!(text(&session).contains("Editable PDF text"));
    run(&mut session, "text.insert", json!({"text": "Changed: "}));
    assert_eq!(run(&mut session, "file.save", json!({}))["saved"], false);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    let copy = dir.join("converted.docx");
    run(&mut session, "file.saveAs", json!({"path": copy.to_string_lossy()}));
    let reopened = crate::io::open_path(&copy).unwrap();
    assert!(reopened.plain_text(StoryRef::Body).starts_with("Changed: Editable PDF text"));
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(copy).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn pdf_import_sniffs_renamed_files_and_failed_open_preserves_the_current_document() {
    let bytes = wordcraft_pdf::export(&Document::from_text("PDF content"), &Default::default()).unwrap();
    let data = base64_encode(&bytes);
    let mut session = s();
    for name in ["renamed.txt", "renamed.docx", "renamed.html", "no-extension", "scan.PDF"] {
        let result = run(&mut session, "file.open", json!({"path": name, "data": data}));
        assert_eq!(result["import"]["format"], "pdf", "{name}");
        assert!(session.path.is_none() && session.dirty, "{name}");
        assert!(text(&session).contains("PDF content"));
    }
    let before = text(&session);
    let invalid = base64_encode(b"%PDF-corrupt");
    assert!(session.run("file.open", &json!({"path": "broken.pdf", "data": invalid})).is_err());
    assert_eq!(text(&session), before);
    assert!(session.dirty && session.path.is_none());
    assert!(crate::io::OPEN_EXTS.contains(&"pdf"));
}

#[test]
fn cell_margins_for_the_table_or_one_cell() {
    let mut s = Session::new(Document::new());
    s.run("insert.table", &json!({"rows": 1, "cols": 2})).unwrap();
    s.run("table.cellMargins", &json!({"top": 8, "left": 0, "bottom": 8, "right": 20, "cell": true})).unwrap();
    let t = s.doc.body[0].as_table().unwrap();
    assert_eq!(t.rows[0].cells[0].props.margins, Some([8.0, 0.0, 8.0, 20.0]));
    assert_eq!(t.rows[0].cells[1].props.margins, None, "only the caret cell");
    assert_eq!(t.props.cell_margins, None);
    s.run("table.cellMargins", &json!({"top": 4})).unwrap();
    assert_eq!(s.doc.body[0].as_table().unwrap().props.cell_margins, Some([4.0, 5.4, 0.0, 5.4]));
    // Outside a table there is nothing to set.
    let mut s = Session::new(Document::new());
    assert!(s.run("table.cellMargins", &json!({"cell": true})).is_err());
}

/// The bundled face `file` of family `original` renamed `family` (as long as `original`), so it
/// is a font the database doesn't have.
fn renamed(file: &str, original: &str, family: &str) -> Vec<u8> {
    assert_eq!(family.len(), original.len());
    let mut data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts").join(file)).unwrap();
    let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
    for (from, to) in [(utf16(original), utf16(family)), (original.as_bytes().to_vec(), family.as_bytes().to_vec())] {
        let mut i = 0;
        while let Some(at) = data[i..].windows(from.len()).position(|w| w == from) {
            data[i + at..i + at + to.len()].copy_from_slice(&to);
            i += at + to.len();
        }
    }
    data
}

/// A .docx whose text is set in `family`, embedding `font` through the font table (#1171): as an
/// obfuscated `.odttf` when `key` is a real GUID, as a plain `.ttf` when it is all zeros.
fn docx_embedding(family: &str, font: Option<(&[u8], &str)>, text: &str) -> Vec<u8> {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const PR: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
    let rels = |items: &[(&str, &str)]| {
        let list: String = items.iter().map(|(t, g)| format!(r#"<Relationship Id="rId{t}" Type="{R}/{t}" Target="{g}"/>"#)).collect();
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{PR}">{list}</Relationships>"#)
    };
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="ttf" ContentType="application/x-font-ttf"/><Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#.to_vec(),
        ),
        ("_rels/.rels".into(), rels(&[("officeDocument", "word/document.xml")]).into_bytes()),
        ("word/_rels/document.xml.rels".into(), rels(&[("fontTable", "fontTable.xml")]).into_bytes()),
        (
            "word/document.xml".into(),
            format!(r#"<w:document xmlns:w="{W}"><w:body><w:p><w:r><w:rPr><w:rFonts w:ascii="{family}" w:hAnsi="{family}"/><w:sz w:val="20"/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#).into_bytes(),
        ),
    ];
    let mut table = format!(r#"<w:fonts xmlns:w="{W}" xmlns:r="{R}"><w:font w:name="{family}">"#);
    if let Some((data, key)) = font {
        let digits = key.replace(['{', '}', '-'], "");
        let hex: Vec<u8> = (0..16).map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).unwrap()).collect();
        let zero = hex.iter().all(|b| *b == 0);
        let mut data = data.to_vec();
        // Obfuscation (ECMA-376 Part 1, 17.8.1): the first 32 bytes XOR the key's bytes, reversed.
        for i in 0..32 {
            data[i] ^= hex[15 - i % 16];
        }
        let ext = if zero { "ttf" } else { "odttf" };
        table.push_str(&format!(r#"<w:embedRegular r:id="rIdfont" w:fontKey="{key}" w:subsetted="0"/>"#));
        files.push(("word/_rels/fontTable.xml.rels".into(), rels(&[("font", &format!("fonts/font1.{ext}"))]).into_bytes()));
        files.push((format!("word/fonts/font1.{ext}"), data));
    }
    table.push_str("</w:font></w:fonts>");
    files.push(("word/fontTable.xml".into(), table.into_bytes()));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut zip, &bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn import_report_preserves_embedded_docx_font_loading() {
    const TEXT: &str = "illicit little lilies fill tiny jill";
    const FAMILY: &str = "JetBrains Rept";
    let end_x = |doc| Session::new(doc).layout().caret_on(&Pos::body(0, TEXT.len()), 0).unwrap().x;
    let mono = end_x(crate::io::open_bytes("mono.docx", &docx_embedding("JetBrains Mono", None, TEXT)).unwrap());
    let missing = crate::io::open_bytes_report("missing.docx", &docx_embedding(FAMILY, None, TEXT), None).unwrap();
    assert!((end_x(missing.doc) - mono).abs() > 1.0, "the test family must initially use a fallback");
    let font = renamed("JetBrainsMono-Regular.ttf", "JetBrains Mono", FAMILY);
    let bytes = docx_embedding(FAMILY, Some((&font, "{00000000-0000-0000-0000-000000000000}")), TEXT);
    let opened = crate::io::open_bytes_report("embedded.docx", &bytes, None).unwrap();
    assert!(opened.pdf.is_none(), "a DOCX must not become a PDF conversion");
    assert!(!opened.encrypted);
    assert!((end_x(opened.doc) - mono).abs() < 0.01, "the report-based open path must load the embedded font");
}
