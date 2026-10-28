//! Edit Pleading (`law.pleadings.edit`, `law.pleadings.current`), saved courts
//! (`law.courts.*`) and contacts (`law.contacts.read`).

use serde_json::{Value, json};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;

use super::tests::{err, law, run};
use crate::pleadings::{NOT_EDITABLE, PROPERTY, STRUCTURE, TAG};

fn firm() -> Value {
    json!({
        "firmName": "Example Law Firm LLC",
        "address1": "100 Main Street",
        "city": "Kirksville",
        "state": "MO",
        "zip": "63501",
        "phone": "660-555-0100",
        "email": "attorney@example.com",
        "attorneyName": "Jane Q. Lawyer",
        "barNumber": "12345"
    })
}

/// A motion for a client whose name is misspelled, against a pro se respondent.
fn motion(client: &str) -> Value {
    json!({
        "type": "motion",
        "title": "Motion to Continue",
        "county": "Adair",
        "caseNumber": "26AR-DR00001",
        "firstSide": {"names": [{"name": client, "client": true}], "role": "Petitioner"},
        "secondSide": {"names": [{"name": "John Doe", "proSe": true, "address": "1 Elm St, Kirksville, MO 63501"}], "role": "Respondent"},
        "filingDate": "2026-10-28",
        "firm": firm()
    })
}

fn pleading(v: Value) -> Session {
    let mut s = law();
    run(&mut s, "law.pleadings.new", v);
    s
}

fn body(doc: &Document) -> String {
    doc.plain_text(StoryRef::Body)
}

/// Each tagged control: (tag without the prefix, text, title).
fn tags(doc: &Document) -> Vec<(String, String, String)> {
    doc.control_ranges(StoryRef::Body)
        .into_iter()
        .filter_map(|r| Some((r.control.tag.strip_prefix(TAG)?.to_string(), doc.control_text(&r), r.control.title.clone())))
        .collect()
}

#[test]
fn new_pleading_tags_every_value() {
    let s = pleading(motion("Jane Deo"));
    let t = tags(&s.doc);
    let has = |key: &str, text: &str| assert!(t.iter().any(|(k, x, _)| k == key && x == text), "{key} = {text:?} in {t:?}");
    has("courtType", "CIRCUIT");
    has("county", "Adair");
    has("caseNumber", "26AR-DR00001");
    has("s1.name.0:up", "JANE DEO");
    has("s1.name.0", "Jane Deo");
    has("s2.name.0:up", "JOHN DOE");
    has("s1.role:side", "Petitioner");
    has("s1.role:clients", "Petitioner");
    has("title", "Motion to Continue");
    has("date:long", "28th day of October, 2026");
    has("s2.name.0", "John Doe");
    has("s2.address.0", "1 Elm St, Kirksville, MO 63501");
    // Each control's title holds the value as typed.
    for (k, _, title) in &t {
        if k.starts_with("s1.name.0") {
            assert_eq!(title, "Jane Deo");
        }
    }
    assert!(t.iter().all(|(k, _, _)| k != "division"), "no division line without a division");
    // Locked against deletion, not against typing.
    for r in s.doc.control_ranges(StoryRef::Body) {
        assert!(!r.control.lock.can_delete() && r.control.lock.can_edit(), "{}", r.control.tag);
    }
    // The shape, with no names in it.
    let prop = s.doc.custom_props.iter().find(|p| p.name == PROPERTY).expect("the property");
    assert!(!prop.value.contains("Deo") && !prop.value.contains("Doe"), "{}", prop.value);
    let shape: Value = serde_json::from_str(&prop.value).expect("json");
    assert_eq!(shape["sides"], json!(["c", "p"]));
}

#[test]
fn current_reads_the_form_back() {
    let mut s = pleading(motion("Jane Deo"));
    let c = run(&mut s, "law.pleadings.current", json!({}));
    assert_eq!(c["type"], "motion");
    assert_eq!(c["title"], "Motion to Continue");
    assert_eq!(c["county"], "Adair");
    assert_eq!(c["caseNumber"], "26AR-DR00001");
    assert_eq!(c["filingDate"], "2026-10-28");
    assert_eq!(c["firstSide"]["names"][0], json!({"name": "Jane Deo", "client": true, "proSe": false}));
    assert_eq!(c["secondSide"]["names"][0]["address"], "1 Elm St, Kirksville, MO 63501");
    assert_eq!(c["secondSide"]["role"], "Respondent");
}

#[test]
fn edit_fixes_a_name_everywhere_in_one_undo_step() {
    let mut s = pleading(motion("Jane Deo"));
    // The person's own words stay.
    run(&mut s, "select.text", json!({"text": "[Grounds.]"}));
    run(&mut s, "text.insert", json!({"text": "The hearing conflicts with trial."}));
    let before = s.doc.clone();
    let steps = s.undo_depth();
    let mut v = run(&mut s, "law.pleadings.current", json!({}));
    v["firstSide"]["names"][0]["name"] = json!("Jane Doe");
    let r = run(&mut s, "law.pleadings.edit", v);
    assert_eq!(r["updated"], 2, "the caption and COMES NOW: {r}");
    assert_eq!(r["missing"], json!([]), "{r}");
    let text = body(&s.doc);
    assert!(!text.contains("Deo") && !text.contains("DEO"), "{text}");
    assert!(text.contains("JANE DOE") && text.contains("COMES NOW Jane Doe, Petitioner"), "{text}");
    assert!(text.contains("The hearing conflicts with trial."));
    assert_eq!(s.undo_depth(), steps + 1, "one step");
    s.run("edit.undo", &json!({})).expect("undo");
    assert_eq!(s.doc, before, "one undo brings the misspelling back");
}

#[test]
fn edit_changes_the_title_and_its_property() {
    let mut s = pleading(motion("Jane Doe"));
    assert_eq!(s.doc.core.title, "Motion to Continue");
    let mut v = run(&mut s, "law.pleadings.current", json!({}));
    v["title"] = json!("Motion for Continuance");
    v["county"] = json!("Macon");
    run(&mut s, "law.pleadings.edit", v);
    assert_eq!(s.doc.core.title, "Motion for Continuance");
    let text = body(&s.doc);
    assert!(text.contains("IN THE CIRCUIT COURT OF Macon COUNTY"), "{text}");
    assert!(text.contains("a copy of this Motion for Continuance was sent to John Doe"), "{text}");
    assert!(!text.contains("Motion to Continue"));
}

#[test]
fn a_petition_title_case_and_verification_follow() {
    let mut v = motion("jane doe");
    v["type"] = json!("petition");
    v["title"] = json!("PETITION FOR DISSOLUTION OF MARRIAGE");
    v["verified"] = json!(true);
    v["secondSide"] = json!({"names": ["John Doe"], "role": "Respondent"});
    let mut s = pleading(v);
    let mut c = run(&mut s, "law.pleadings.current", json!({}));
    c["title"] = json!("PETITION FOR LEGAL SEPARATION");
    c["county"] = json!("MACON");
    run(&mut s, "law.pleadings.edit", c);
    let text = body(&s.doc);
    assert!(text.contains("and for Petition for Legal Separation states"), "{text}");
    assert!(text.contains("County of Macon"), "{text}");
}

#[test]
fn edit_refuses_a_different_shape() {
    let mut s = pleading(motion("Jane Doe"));
    let c = run(&mut s, "law.pleadings.current", json!({}));
    let mut more = c.clone();
    more["secondSide"]["names"].as_array_mut().expect("names").push(json!("Mary Roe"));
    let mut kind = c.clone();
    kind["type"] = json!("answer");
    let mut division = c;
    division["division"] = json!("2");
    for v in [more, kind, division] {
        let e = err(&mut s, "law.pleadings.edit", v);
        assert!(e.to_string().contains(STRUCTURE), "{e}");
    }
}

#[test]
fn edit_refuses_a_document_new_pleading_did_not_write() {
    let mut s = law();
    run(&mut s, "text.insert", json!({"text": "A letter."}));
    for v in [json!({}), motion("Jane Doe")] {
        let e = err(&mut s, "law.pleadings.edit", v);
        assert!(e.to_string().contains(NOT_EDITABLE), "{e}");
    }
    let e = err(&mut s, "law.pleadings.current", json!({}));
    assert!(e.to_string().contains(NOT_EDITABLE), "{e}");
}

#[test]
fn edit_without_params_asks_for_the_form() {
    let mut s = pleading(motion("Jane Doe"));
    s.ui_requests.clear();
    assert_eq!(run(&mut s, "law.pleadings.edit", json!({})), json!({"pending": "editPleading"}));
    assert_eq!(s.ui_requests, vec![json!({"addin": "law", "open": "editPleading"})]);
}

#[test]
fn tags_and_shape_survive_docx_and_edit_works_after() {
    let s = pleading(motion("Jane Deo"));
    let bytes = wordcraft_engine::io::save_bytes("pleading.docx", &s.doc).expect("save");
    let reopened = wordcraft_engine::io::open_bytes("pleading.docx", &bytes).expect("reopen");
    assert_eq!(tags(&reopened).len(), tags(&s.doc).len());
    let mut s2 = law();
    s2.set_document(reopened);
    let mut v = run(&mut s2, "law.pleadings.current", json!({}));
    assert_eq!(v["firstSide"]["names"][0]["name"], "Jane Deo");
    v["firstSide"]["names"][0]["name"] = json!("Jane Doe");
    run(&mut s2, "law.pleadings.edit", v);
    assert!(!body(&s2.doc).contains("DEO"));
}

/// Saved courts, all in one test: the list is the shared store's.
#[test]
fn saved_courts() {
    let mut s = law();
    let court =
        run(&mut s, "law.courts.save", json!({"name": "Zz Adair County Circuit", "courtType": "CIRCUIT", "county": "ADAIR", "division": "II"}));
    let id = court["id"].as_str().expect("id").to_string();
    assert_eq!(id, "zz-adair-county-circuit");
    assert_eq!(court["state"], "MISSOURI");
    assert_eq!(court["profile"], crate::pleadings::PROFILE);
    let list = run(&mut s, "law.courts.list", json!({}));
    assert!(list.as_array().expect("list").iter().any(|c| c["id"] == json!(id)));
    // Replace by id, and by name.
    run(&mut s, "law.courts.save", json!({"id": id, "name": "Zz Adair County Circuit", "courtType": "CIRCUIT", "county": "ADAIR", "division": "I"}));
    let again =
        run(&mut s, "law.courts.save", json!({"name": "zz adair county circuit", "courtType": "CIRCUIT", "county": "ADAIR", "division": "I"}));
    assert_eq!(again["id"], json!(id), "same name, same court");
    // Refused: unknown fields, long values, missing ones.
    for bad in [
        json!({"name": "X", "courtType": "CIRCUIT", "county": "ADAIR", "judge": "Smith"}),
        json!({"name": "X", "courtType": "CIRCUIT", "county": "x".repeat(201)}),
        json!({"name": "X", "courtType": "CIRCUIT"}),
        json!({"name": " ", "courtType": "CIRCUIT", "county": "ADAIR"}),
        json!({"id": "no-such-court", "name": "X", "courtType": "CIRCUIT", "county": "ADAIR"}),
    ] {
        err(&mut s, "law.courts.save", bad);
    }
    // `court` fills the caption, with its division line.
    let mut v = motion("Jane Doe");
    v.as_object_mut().expect("params").remove("county");
    v["court"] = json!(id);
    let mut p = pleading(v.clone());
    let text = body(&p.doc);
    assert!(text.contains("COURT OF ADAIR COUNTY") && text.contains("Division: I"), "{text}");
    assert_eq!(run(&mut p, "law.pleadings.current", json!({}))["division"], "I");
    // `saveCourt` saves the form's court.
    let mut w = motion("Jane Doe");
    w["county"] = json!("ZZ MACON");
    w["saveCourt"] = json!("Zz Macon County Circuit");
    let r = run(&mut law(), "law.pleadings.new", w);
    assert_eq!(r["court"]["county"], "ZZ MACON");
    let macon = r["court"]["id"].as_str().expect("id").to_string();
    // Delete.
    for id in [&id, &macon] {
        assert_eq!(run(&mut s, "law.courts.delete", json!({"id": id}))["deleted"], json!(id));
    }
    err(&mut s, "law.courts.delete", json!({"id": id}));
    err(&mut s, "law.pleadings.new", v);
    // The cap.
    let start = crate::cmd::store().courts().len();
    for i in start..crate::courts::MAX_COURTS {
        run(&mut s, "law.courts.save", json!({"name": format!("Zz cap {i}"), "courtType": "CIRCUIT", "county": "ADAIR"}));
    }
    let e = err(&mut s, "law.courts.save", json!({"name": "Zz one too many", "courtType": "CIRCUIT", "county": "ADAIR"}));
    assert!(e.to_string().contains("500"), "{e}");
    let keep: Vec<_> = crate::cmd::store().courts().iter().filter(|c| !c.name.starts_with("Zz cap")).cloned().collect();
    crate::cmd::store().set_courts(keep).expect("clear");
}

#[test]
fn saved_courts_are_a_file_beside_the_profiles() {
    let dir = super::tests::temp_dir("courts");
    let mut store = crate::store::Store::new(Some(dir.clone()));
    let court: crate::courts::Court =
        serde_json::from_value(json!({"id": "adair", "name": "Adair", "courtType": "CIRCUIT", "county": "ADAIR"})).expect("court");
    store.set_courts(vec![court.clone()]).expect("save");
    assert!(dir.join("courts.json").exists());
    let reopened = crate::store::Store::new(Some(dir.clone()));
    assert_eq!(reopened.courts(), &[court]);
    assert_eq!(reopened.list().len(), crate::store::bundled().len(), "courts.json is not a profile");
    let _ = std::fs::remove_dir_all(&dir);
}

const CARDS: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:Doe;John;Q.;;Jr.\r\nFN:John Q. Doe Jr.\r\nORG:Doe Farms\\, LLC;Sales\r\nADR;TYPE=HOME:;;9 Home Rd;Macon;MO;63552;USA\r\nitem1.ADR;TYPE=WORK:;Suite 2;100 Main St;Kirksville;MO;63501;\r\nEMAIL;TYPE=INTERNET:john@example.com\r\nTEL;TYPE=CELL:660-555-0199\r\nEND:VCARD\r\nBEGIN:VCARD\r\nVERSION:4.0\r\nN:Roe;Mary;;;\r\nADR:;;12 Long Street With A Very Long Name That Wr\r\n aps;Kirksville;MO;63501;\r\nTEL;VALUE=uri:tel:+1-660-555-0100\r\nEND:VCARD\r\nBEGIN:VCARD\r\nVERSION:4.0\r\nORG:Adair County Circuit Clerk\r\nEND:VCARD\r\nBEGIN:VCARD\r\nVERSION:3.0\r\nNOTE:no name\r\nEND:VCARD\r\n";

#[test]
fn vcards_parse() {
    let c = crate::contacts::parse(CARDS).expect("parse");
    assert_eq!(c.len(), 3, "the card with no name is left out");
    assert_eq!(c[0].name, "John Q. Doe Jr.");
    assert_eq!(c[0].organization, "Doe Farms, LLC, Sales");
    assert_eq!(c[0].address, "Suite 2\n100 Main St\nKirksville, MO 63501", "the work address wins");
    assert_eq!(c[0].email, "john@example.com");
    assert_eq!(c[0].phone, "660-555-0199");
    assert_eq!(c[1].name, "Mary Roe", "from N when FN is missing");
    assert_eq!(c[1].address, "12 Long Street With A Very Long Name That Wraps\nKirksville, MO 63501", "a folded line");
    assert_eq!(c[1].phone, "+1-660-555-0100");
    assert_eq!(c[2].name, "Adair County Circuit Clerk", "an organization's card");
}

#[test]
fn contacts_read_files_and_refuse_big_ones() {
    let dir = super::tests::temp_dir("contacts");
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("people.vcf");
    std::fs::write(&path, CARDS).expect("write");
    let mut s = law();
    let r = run(&mut s, "law.contacts.read", json!({"path": path.to_str().expect("utf8")}));
    assert_eq!(
        r[1],
        json!({"name": "Mary Roe", "organization": "", "address": "12 Long Street With A Very Long Name That Wraps\nKirksville, MO 63501", "email": "", "phone": "+1-660-555-0100"})
    );
    let big = dir.join("big.vcf");
    std::fs::write(&big, "x".repeat((crate::contacts::MAX_BYTES + 1) as usize)).expect("write");
    err(&mut s, "law.contacts.read", json!({"path": big.to_str().expect("utf8")}));
    let many = "BEGIN:VCARD\nFN:A\nEND:VCARD\n".repeat(crate::contacts::MAX_CARDS + 1);
    assert!(crate::contacts::parse(&many).is_err());
    // Confined to other roots, the file is out of reach.
    let other = super::tests::temp_dir("contacts-root");
    std::fs::create_dir_all(&other).expect("dir");
    let roots = wordcraft_engine::file_access::AutomationRoots::new(Some(&other), Some(&other)).expect("roots").expect("some");
    let confined =
        wordcraft_engine::file_access::confine(Some(&roots), || s.run("law.contacts.read", &json!({"path": path.to_str().expect("utf8")})));
    assert!(confined.is_err(), "outside the read root");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);
}
