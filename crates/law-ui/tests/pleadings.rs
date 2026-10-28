//! The New Pleading window: Create waits for the required fields, makes the document blank first
//! (File › New, with its Save Changes prompt), and writes the pleading.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Mutex;

use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;
use wordcraft_law::MODULE;
use wordcraft_law_ui::LawAddIn;
use wordcraft_law_ui::pleading::{PleadingForm, firm_from};
use wordcraft_ui_egui::dialogs::Dialog;
use wordcraft_ui_egui::{Services, WordApp};

/// The firm details file sits beside the process-wide profile store; one test at a time.
static LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("wc-ui-law-pleadings-{}-{n}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    wordcraft_law::set_profiles_dir(Some(dir));
    guard
}

fn law_app() -> WordApp {
    let mut s = Session::new(Document::new());
    s.prefs.modules.insert(MODULE.into(), true);
    let mut a = WordApp::new(s, Services::default());
    a.add_addin(Box::new(LawAddIn::default()));
    a
}

fn harness(a: WordApp) -> egui_kittest::Harness<'static, WordApp> {
    egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        a,
    )
}

fn form(a: &mut WordApp) -> &mut PleadingForm {
    a.addin_mut::<LawAddIn>(MODULE).unwrap().state.pleading.as_mut().expect("the New Pleading window is open")
}

fn is_open(a: &WordApp) -> bool {
    a.addin::<LawAddIn>(MODULE).unwrap().state.pleading.is_some()
}

fn fill(f: &mut PleadingForm) {
    f.county = "ADAIR".into();
    f.case_number = "26AR-DR00012".into();
    f.first.parties[0].name = "JANE DOE".into();
    f.second.parties[0].name = "JOHN DOE".into();
    f.filing_date = "2026-10-15".into();
    f.firm = firm_from(&json!({
        "firmName": "Nolan Law Firm LLC", "address1": "101 Main St", "city": "Kirksville", "state": "MO", "zip": "63501",
        "phone": "660-555-0100", "email": "office@example.com", "attorneyName": "Pat Nolan", "barNumber": "12345",
    }));
}

fn create_disabled(h: &egui_kittest::Harness<'static, WordApp>) -> bool {
    h.get_by_label("Create").accesskit_node().is_disabled()
}

fn body(a: &WordApp) -> String {
    a.session.doc.plain_text(StoryRef::Body)
}

/// The caret sits on the first placeholder, collapsed.
fn caret_on_placeholder(a: &WordApp) -> bool {
    let sel = &a.session.sel;
    sel.anchor == sel.focus
        && a.session.doc.para(sel.focus.story, &sel.focus.path).and_then(|p| p.text.get(sel.focus.off..)).is_some_and(|t| t.starts_with('['))
}

#[test]
fn create_stays_disabled_until_the_required_fields_are_set() {
    let _lock = serial();
    let mut a = law_app();
    // What `law.pleadings.new` with no params asks of the interface.
    a.session.ui_requests.push(json!({"addin": MODULE, "open": "newPleading"}));
    a.run("select.all", json!({})).unwrap();
    assert!(is_open(&a));
    assert_eq!(form(&mut a).title, "Motion", "the title starts as the type's");
    let today = form(&mut a).filing_date.clone();
    assert_eq!(today, wordcraft_law_ui::pleading::today(), "the filing date starts as today");
    assert!(today.len() == 10 && today.as_bytes().get(4) == Some(&b'-') && today.as_bytes().get(7) == Some(&b'-'), "{today}");
    assert_eq!(form(&mut a).params()["filingDate"], today.as_str(), "the date always goes to the command");
    let mut h = harness(a);
    h.run_steps(4);
    assert!(create_disabled(&h));
    assert!(h.query_by_label("Fill in County to create.").is_some(), "the reason is shown");
    fill(form(h.state_mut()));
    h.run_steps(2);
    assert!(!create_disabled(&h));
    let params = form(h.state_mut()).params();
    assert_eq!(params["firstSide"]["names"], json!([{"name": "JANE DOE", "client": true}]), "the first party starts as our client");
    assert_eq!(params["secondSide"]["names"], json!([{"name": "JOHN DOE", "client": false}]));
    form(h.state_mut()).first.parties[0].client = false;
    h.run_steps(2);
    assert!(create_disabled(&h), "someone has to be our client");
    assert!(h.query_by_label("Tick Client beside a party we represent to create.").is_some());
    let f = form(h.state_mut());
    f.first.parties[0].client = true;
    f.second.parties[0].pro_se = true;
    h.run_steps(2);
    assert!(create_disabled(&h), "a pro se party needs a mailing address");
    assert!(h.query_by_label("Fill in the mailing address for JOHN DOE to create.").is_some());
    let f = form(h.state_mut());
    f.second.parties[0].address = "1 Elm Street\nMacon, MO 63552".into();
    let params = f.params();
    assert_eq!(params["secondSide"]["names"][0]["address"], "1 Elm Street, Macon, MO 63552", "one line");
    h.run_steps(2);
    assert!(!create_disabled(&h));
    let f = form(h.state_mut());
    f.second.parties[0].pro_se = false;
    f.filing_date.clear();
    h.run_steps(2);
    assert!(create_disabled(&h), "the filing date is required");
    assert!(h.query_by_label("Fill in Filing date to create.").is_some());
    let f = form(h.state_mut());
    f.filing_date = "2026-10-15".into();
    f.firm = Default::default();
    h.run_steps(2);
    assert!(create_disabled(&h), "the firm details are required");
    assert!(h.query_by_label("Fill in Firm name to create.").is_some());
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(!is_open(h.state()));
}

#[test]
fn create_on_a_clean_document_with_text_runs_file_new_and_writes_the_pleading() {
    let _lock = serial();
    let mut a = law_app();
    a.run("text.insert", json!({"text": "Old notes"})).unwrap();
    a.session.dirty = false;
    let before = a.session.document_id();
    let depth = a.session.undo_depth();
    a.run("law.pleadings.new", json!({})).unwrap();
    assert!(is_open(&a), "no params opens the window");
    assert!(!a.session.dirty && a.session.undo_depth() == depth, "opening it changes nothing");
    fill(form(&mut a));
    let mut h = harness(a);
    h.run_steps(4);
    h.get_by_label("Create").click();
    h.run_steps(4);
    let a = h.state_mut();
    assert!(!is_open(a), "{:?}", a.addin::<LawAddIn>(MODULE).unwrap().state.pleading.as_ref().map(|f| &f.error));
    assert_ne!(a.session.document_id(), before, "a new document");
    let text = body(a);
    assert!(text.contains("IN THE CIRCUIT COURT OF ADAIR COUNTY"), "{text}");
    assert!(!text.contains("Old notes"));
    assert!(caret_on_placeholder(a));
    let firm = a.run("law.firm.get", json!({})).unwrap();
    assert_eq!(firm["firmName"], "Nolan Law Firm LLC", "Remember firm details saved them");
}

/// Unsaved work stays open: File › New opens another document beside it, so Create puts the
/// pleading there without asking to save anything.
#[test]
fn unsaved_work_stays_open_beside_the_new_pleading() {
    let _lock = serial();
    let mut a = law_app();
    a.run("text.insert", json!({"text": "Unsaved work"})).unwrap();
    assert!(a.session.dirty);
    let before = a.session.document_id();
    a.run("law.pleadings.new", json!({})).unwrap();
    fill(form(&mut a));
    let mut h = harness(a);
    h.run_steps(4);
    h.get_by_label("Create").click();
    h.run_steps(4);
    let a = h.state_mut();
    assert!(!matches!(a.dialog, Some(Dialog::SaveChanges { .. })), "nothing to save first");
    assert!(!is_open(a));
    assert_ne!(a.session.document_id(), before, "a new document");
    assert!(body(a).contains("STATE OF MISSOURI"));
    assert!(caret_on_placeholder(a));
    assert!(a.session.is_open(before), "the unsaved document is still open");
    a.session.activate(before).unwrap();
    assert!(a.session.dirty);
    assert_eq!(body(a).trim(), "Unsaved work");
}

fn params(client: &str) -> serde_json::Value {
    json!({
        "type": "motion", "title": "Motion to Continue", "county": "ADAIR", "caseNumber": "26AR-DR00012",
        "firstSide": {"names": [{"name": client, "client": true}], "role": "Petitioner"},
        "secondSide": {"names": ["John Doe"], "role": "Respondent"},
        "filingDate": "2026-10-28",
        "firm": {"firmName": "Nolan Law Firm LLC", "address1": "101 Main St", "city": "Kirksville", "state": "MO", "zip": "63501",
            "phone": "660-555-0100", "email": "office@example.com", "attorneyName": "Pat Nolan", "barNumber": "12345"},
    })
}

/// Edit Pleading opens the form filled from the document; Update fixes the name everywhere.
#[test]
fn edit_pleading_reopens_the_form_and_fixes_a_misspelled_name() {
    let _lock = serial();
    let mut a = law_app();
    a.run("law.pleadings.new", params("Jane Deo")).unwrap();
    a.run("law.pleadings.edit", json!({})).unwrap();
    assert!(is_open(&a));
    let mut h = harness(a);
    h.run_steps(4);
    assert!(h.query_by_label("Update").is_some() && h.query_by_label("Create").is_none());
    let f = form(h.state_mut());
    assert!(f.edit);
    assert_eq!(f.first.parties[0].name, "Jane Deo");
    assert_eq!(f.second.parties[0].name, "John Doe", "a name only in capitals comes back as typed");
    assert_eq!(f.case_number, "26AR-DR00012");
    f.first.parties[0].name = "Jane Doe".into();
    h.run_steps(2);
    h.get_by_label("Update").click();
    h.run_steps(4);
    let a = h.state_mut();
    assert!(!is_open(a), "{:?}", a.addin::<LawAddIn>(MODULE).unwrap().state.pleading.as_ref().map(|f| &f.error));
    let text = body(a);
    assert!(text.contains("JANE DOE") && !text.contains("DEO") && !text.contains("Deo"), "{text}");
}

/// A saved court is offered and fills the court fields; Save this court saves one.
#[test]
fn saved_courts_fill_the_form_and_save_this_court_saves_one() {
    let _lock = serial();
    let mut a = law_app();
    let court =
        a.run("law.courts.save", json!({"name": "Adair County Circuit", "courtType": "CIRCUIT", "county": "ADAIR", "division": "II"})).unwrap();
    a.run("law.pleadings.new", json!({})).unwrap();
    let mut h = harness(a);
    h.run_steps(4);
    assert!(h.query_by_label("Saved court").is_some());
    let f = form(h.state_mut());
    assert_eq!(f.courts.len(), 1);
    f.use_court(&court);
    assert_eq!((f.court_type.as_str(), f.county.as_str(), f.division.as_str()), ("CIRCUIT", "ADAIR", "II"));
    f.county = "MACON".into();
    f.save_court = true;
    f.court_name = f.default_court_name();
    assert_eq!(f.court_name, "Macon County Circuit");
    assert_eq!(f.params()["saveCourt"], "Macon County Circuit");
    assert_eq!(f.params()["division"], "II");
    fill(f);
    f.county = "MACON".into();
    h.run_steps(2);
    h.get_by_label("Create").click();
    h.run_steps(4);
    let a = h.state_mut();
    assert!(body(a).contains("Division: II"));
    let names: Vec<String> =
        a.run("law.courts.list", json!({})).unwrap().as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["Adair County Circuit", "Macon County Circuit"]);
}

/// From contacts… fills a party's name and mailing address from a .vcf file.
#[test]
fn from_contacts_fills_a_party() {
    let _lock = serial();
    let dir = std::env::temp_dir().join(format!("wc-ui-law-vcf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("one.vcf");
    std::fs::write(&path, "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Mary Roe\r\nADR:;;12 Oak St;Kirksville;MO;63501;\r\nEND:VCARD\r\n").unwrap();
    let picked = path.to_str().unwrap().to_string();
    let mut s = Session::new(Document::new());
    s.prefs.modules.insert(MODULE.into(), true);
    let mut a = WordApp::new(s, Services { pick_open: Some(Box::new(move |_| Some(picked.clone()))), ..Default::default() });
    a.add_addin(Box::new(LawAddIn::default()));
    a.run("law.pleadings.new", json!({})).unwrap();
    let mut h = harness(a);
    h.run_steps(4);
    form(h.state_mut()).second.parties[0].pro_se = true;
    h.run_steps(2);
    h.get_all_by_label("From contacts…").nth(1).expect("the second side's button").click();
    h.run_steps(4);
    let f = form(h.state_mut());
    assert_eq!(f.second.parties[0].name, "Mary Roe");
    assert_eq!(f.second.parties[0].address, "12 Oak St\nKirksville, MO 63501");
    let _ = std::fs::remove_dir_all(&dir);
}
