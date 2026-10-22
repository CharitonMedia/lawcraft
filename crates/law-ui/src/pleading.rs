//! The New Pleading window (`law.pleadings.new` with no params asks for it) and the firm details
//! it shares with File › Options › LawCraft. English only, like the options page.

use egui::{RichText, Ui};
use serde_json::{Map, Value, json};
use wordcraft_ui_egui::theme::semibold;

use crate::Ctx;

/// The error `law.pleadings.new` gives on a document that isn't blank: the window then runs File ›
/// New first, as the File menu does.
const NOT_BLANK: &str = "New Pleading writes into a blank document";

/// The firm block's fields: (key, label, required).
pub const FIRM_FIELDS: [(&str, &str, bool); 11] = [
    ("firmName", "Firm name", true),
    ("address1", "Address", true),
    ("address2", "Address line 2", false),
    ("city", "City", true),
    ("state", "State", true),
    ("zip", "ZIP code", true),
    ("phone", "Phone", true),
    ("fax", "Fax", false),
    ("email", "Email", true),
    ("attorneyName", "Attorney name", true),
    ("barNumber", "MO Bar No.", true),
];

/// Firm details as typed, one value per [`FIRM_FIELDS`] entry.
pub type Firm = [String; 11];

/// Firm details from a saved firm block (`law.firm.get`); missing fields are empty.
pub fn firm_from(v: &Value) -> Firm {
    let mut f = Firm::default();
    for ((key, _, _), value) in FIRM_FIELDS.iter().zip(f.iter_mut()) {
        *value = v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    }
    f
}

/// The firm block `law.firm.set` and `law.pleadings.new` take; empty fields are left out.
pub fn firm_json(f: &Firm) -> Value {
    let mut m = Map::new();
    for ((key, _, _), value) in FIRM_FIELDS.iter().zip(f) {
        if !value.trim().is_empty() {
            m.insert((*key).to_string(), json!(value.trim()));
        }
    }
    Value::Object(m)
}

/// The first required firm field that is empty.
fn firm_missing(f: &Firm) -> Option<&'static str> {
    FIRM_FIELDS.iter().zip(f).find(|((_, _, required), value)| *required && value.trim().is_empty()).map(|((_, label, _), _)| *label)
}

/// The firm fields as a form; whether any changed.
fn firm_fields(ui: &mut Ui, f: &mut Firm, salt: &str) -> bool {
    let mut changed = false;
    egui::Grid::new(salt).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        for ((_, label, _), value) in FIRM_FIELDS.iter().zip(f.iter_mut()) {
            let l = ui.label(*label);
            changed |= ui.add(egui::TextEdit::singleline(value).desired_width(260.0)).labelled_by(l.id).changed();
            ui.end_row();
        }
    });
    changed
}

/// The kinds of pleading: (type, button label, title the form starts with).
const TYPES: [(&str, &str, &str); 4] =
    [("motion", "Motion", "Motion"), ("notice", "Notice", "Notice of Hearing"), ("answer", "Answer", "Answer"), ("petition", "Petition", "Petition")];

/// The roles each side offers before Other, which takes a typed role.
const FIRST_ROLES: [&str; 2] = ["Petitioner", "Plaintiff"];
const SECOND_ROLES: [&str; 2] = ["Respondent", "Defendant"];

/// Most names on one side (the command's limit).
const MAX_NAMES: usize = 20;

/// One party on a side, as typed.
#[derive(Clone, Debug, Default)]
pub struct PartyRow {
    pub name: String,
    /// We represent this party.
    pub client: bool,
    /// No counsel: served by USPS at `address`.
    pub pro_se: bool,
    pub address: String,
}

/// One side of the caption: its parties and their role.
#[derive(Clone, Debug)]
pub struct SideForm {
    pub parties: Vec<PartyRow>,
    /// A role from the list, or what was typed for Other.
    pub role: String,
    /// The role is Other, typed in.
    pub other: bool,
}

impl SideForm {
    /// One empty party, ticked as our client or not, in `role`.
    fn new(role: &str, client: bool) -> Self {
        Self { parties: vec![PartyRow { client, ..Default::default() }], role: role.into(), other: false }
    }

    /// The parties with a name.
    fn filled(&self) -> impl Iterator<Item = &PartyRow> {
        self.parties.iter().filter(|p| !p.name.trim().is_empty())
    }

    /// The side as `law.pleadings.new` takes it. A petition is served with the summons, so
    /// pro se parties get no mailing there.
    fn json(&self, petition: bool) -> Value {
        let list: Vec<Value> = self
            .filled()
            .map(|p| {
                let mut o = json!({"name": p.name.trim(), "client": p.client});
                if p.pro_se && !p.client && !petition {
                    o["proSe"] = json!(true);
                    o["address"] = json!(names(&p.address).join(", "));
                }
                o
            })
            .collect();
        json!({"names": list, "role": self.role.trim()})
    }
}

/// What the New Pleading window holds while it is open.
#[derive(Clone, Debug)]
pub struct PleadingForm {
    /// `motion`, `notice`, `answer` or `petition`.
    pub kind: &'static str,
    pub title: String,
    pub court_type: String,
    pub county: String,
    pub case_number: String,
    pub first: SideForm,
    pub second: SideForm,
    /// `YYYY-MM-DD`, prefilled with today so the person sees it and can correct it.
    pub filing_date: String,
    /// Petition only.
    pub verified: bool,
    pub firm: Firm,
    pub remember_firm: bool,
    /// Firm details were saved already (their section starts closed).
    pub firm_saved: bool,
    /// Why the last Create didn't go through.
    pub error: Option<String>,
    /// File › New is waiting on its Save Changes prompt: the document that was open then.
    pub pending: Option<u64>,
    /// The saved firm details have been read in.
    loaded: bool,
}

impl Default for PleadingForm {
    /// A motion in circuit court, the firm's client as petitioner.
    fn default() -> Self {
        Self {
            kind: "motion",
            title: "Motion".into(),
            court_type: "CIRCUIT".into(),
            county: String::new(),
            case_number: String::new(),
            first: SideForm::new("Petitioner", true),
            second: SideForm::new("Respondent", false),
            filing_date: today(),
            verified: false,
            firm: Firm::default(),
            remember_firm: true,
            firm_saved: false,
            error: None,
            pending: None,
            loaded: false,
        }
    }
}

/// Today as `YYYY-MM-DD`.
// ponytail: UTC, the same clock insert.dateTime uses; nothing in the workspace gives the local offset
// without a new dependency or unsafe. The field is editable, so a person late in the evening corrects it.
pub fn today() -> String {
    wordcraft_engine::cmd::now_iso().chars().take(10).collect()
}

/// The non-empty lines of a names field, trimmed.
fn names(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

impl PleadingForm {
    /// Whether the form is for a petition rather than a motion.
    fn petition(&self) -> bool {
        self.kind == "petition"
    }

    /// Why Create has to wait, naming the first thing missing.
    pub fn missing(&self) -> Option<String> {
        let fill = |label: &str| Some(format!("Fill in {label} to create."));
        for (value, label) in [(&self.title, "Title"), (&self.court_type, "Court type"), (&self.county, "County")] {
            if value.trim().is_empty() {
                return fill(label);
            }
        }
        for (side, which) in [(&self.first, "First side"), (&self.second, "Second side")] {
            if side.filled().next().is_none() {
                return fill(&format!("{which} names"));
            }
            if side.role.trim().is_empty() {
                return fill(&format!("{which} role"));
            }
        }
        if !self.first.filled().chain(self.second.filled()).any(|p| p.client) {
            return Some("Tick Client beside a party we represent to create.".into());
        }
        if self.filing_date.trim().is_empty() {
            return fill("Filing date");
        }
        if !self.petition() {
            if self.case_number.trim().is_empty() {
                return fill("Case No.");
            }
            let unaddressed = self.first.filled().chain(self.second.filled()).find(|p| p.pro_se && !p.client && p.address.trim().is_empty());
            if let Some(p) = unaddressed {
                return Some(format!("Fill in the mailing address for {} to create.", p.name.trim()));
            }
        }
        firm_missing(&self.firm).map(|label| format!("Fill in {label} to create."))
    }

    /// The params of `law.pleadings.new`.
    pub fn params(&self) -> Value {
        let mut p = json!({
            "type": self.kind,
            "title": self.title.trim(),
            "courtType": self.court_type.trim(),
            "county": self.county.trim(),
            "caseNumber": self.case_number.trim(),
            "firstSide": self.first.json(self.petition()),
            "secondSide": self.second.json(self.petition()),
            "filingDate": self.filing_date.trim(),
            "firm": firm_json(&self.firm),
        });
        if self.petition() {
            p["verified"] = json!(self.verified);
        }
        p
    }
}

/// `law.pleadings.new` asked for the window.
pub(crate) fn open(law: &mut crate::LawUi, what: &str) {
    if what == "newPleading" && law.pleading.is_none() {
        law.pleading = Some(PleadingForm::default());
    }
}

/// Draw the window while it is open, and finish a Create that waited on File › New.
pub(crate) fn show(app: &mut Ctx<'_>, ctx: &egui::Context) {
    let Some(mut form) = app.law.pleading.take() else { return };
    if !form.loaded {
        form.loaded = true;
        let saved = app.run("law.firm.get", json!({})).unwrap_or_default();
        if saved.as_object().is_some_and(|o| !o.is_empty()) {
            form.firm = firm_from(&saved);
            form.firm_saved = true;
        }
    }
    let mut keep = true;
    if let Some(old) = form.pending {
        if app.session.document_id() != old {
            // File › New went through: the blank document is in place.
            form.pending = None;
            keep = !create(app, &mut form);
        } else if app.dialog.is_none() && !app.file_dialog_open() {
            // The Save Changes prompt (or its Save As) was cancelled: nothing is created and the
            // window stays.
            form.pending = None;
        }
    }
    // While File › New's prompt is up the window steps aside for it.
    if keep && form.pending.is_none() {
        let mut open = true;
        let mut close = false;
        egui::Window::new("New Pleading").id(egui::Id::new("law_new_pleading")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            ui.set_min_width(420.0);
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| form_ui(ui, &mut form));
            if let Some(e) = &form.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            let missing = form.missing();
            if let Some(why) = &missing {
                ui.label(RichText::new(why).small().weak());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(missing.is_none(), egui::Button::new("Create")).clicked() && submit(app, &mut form) {
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        keep = open && !close && keep;
    }
    if keep {
        app.law.pleading = Some(form);
    }
}

/// The New Pleading form's fields.
fn form_ui(ui: &mut Ui, form: &mut PleadingForm) {
    let field = |ui: &mut Ui, label: &str, value: &mut String| {
        let l = ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(260.0)).labelled_by(l.id);
        ui.end_row();
    };
    egui::Grid::new("law_pleading_form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Type");
        ui.horizontal(|ui| {
            for (kind, label, title) in TYPES {
                if ui.selectable_label(form.kind == kind, label).clicked() && form.kind != kind {
                    form.kind = kind;
                    form.title = title.to_string();
                }
            }
        });
        ui.end_row();
        field(ui, "Title", &mut form.title);
        field(ui, "Court type", &mut form.court_type);
        field(ui, "County", &mut form.county);
        field(ui, "Case No.", &mut form.case_number);
    });
    let petition = form.kind == "petition";
    // Our clients are all on one side: ticking Client on one side clears the other's.
    if side_ui(ui, "First side", &FIRST_ROLES, &mut form.first, petition, "law_side_first") {
        form.second.parties.iter_mut().for_each(|p| p.client = false);
    }
    if side_ui(ui, "Second side", &SECOND_ROLES, &mut form.second, petition, "law_side_second") {
        form.first.parties.iter_mut().for_each(|p| p.client = false);
    }
    ui.add_space(6.0);
    egui::Grid::new("law_pleading_form_end").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let l = ui.label("Filing date");
        ui.add(egui::TextEdit::singleline(&mut form.filing_date).hint_text("YYYY-MM-DD").desired_width(260.0)).labelled_by(l.id);
        ui.end_row();
        if petition {
            ui.label("");
            ui.checkbox(&mut form.verified, "Verified");
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Firm details").default_open(!form.firm_saved).show(ui, |ui| {
        firm_fields(ui, &mut form.firm, "law_pleading_firm");
    });
    ui.checkbox(&mut form.remember_firm, "Remember firm details");
}

/// One side: its role (from the list, or Other typed in) and its parties, each with Client and,
/// outside a petition, Pro se with a mailing address. `true` when a Client box was just ticked.
fn side_ui(ui: &mut Ui, heading: &str, roles: &[&str], side: &mut SideForm, petition: bool, salt: &str) -> bool {
    let mut ticked = false;
    ui.add_space(6.0);
    ui.label(RichText::new(heading).font(semibold(13.0)));
    ui.push_id(salt, |ui| {
        ui.horizontal(|ui| {
            let l = ui.label("Role");
            let shown = if side.other { "Other".to_string() } else { side.role.clone() };
            egui::ComboBox::from_id_salt("role")
                .selected_text(shown)
                .width(140.0)
                .show_ui(ui, |ui| {
                    for role in roles {
                        if ui.selectable_label(!side.other && side.role == *role, *role).clicked() {
                            side.role = (*role).to_string();
                            side.other = false;
                        }
                    }
                    if ui.selectable_label(side.other, "Other").clicked() && !side.other {
                        side.other = true;
                        side.role.clear();
                    }
                })
                .response
                .labelled_by(l.id);
            if side.other {
                ui.add(egui::TextEdit::singleline(&mut side.role).hint_text("Role").desired_width(160.0)).labelled_by(l.id);
            }
        });
        let count = side.parties.len();
        let mut remove = None;
        for (i, p) in side.parties.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.name).hint_text("Name").desired_width(220.0));
                    if ui.checkbox(&mut p.client, "Client").changed() && p.client {
                        p.pro_se = false;
                        ticked = true;
                    }
                    if !petition {
                        ui.add_enabled(!p.client, egui::Checkbox::new(&mut p.pro_se, "Pro se"));
                    }
                    if count > 1 && ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                });
                if p.pro_se && !p.client && !petition {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.add(egui::TextEdit::multiline(&mut p.address).hint_text("Mailing address").desired_rows(2).desired_width(260.0));
                    });
                }
            });
        }
        if let Some(i) = remove {
            side.parties.remove(i);
        }
        if side.parties.len() < MAX_NAMES && ui.small_button("Add name").clicked() {
            side.parties.push(PartyRow::default());
        }
    });
    ticked
}

/// Create: save the firm details if asked, make sure the document is blank (File › New, with its
/// Save Changes prompt), then write the pleading. `true`: done, close the window.
fn submit(app: &mut Ctx<'_>, form: &mut PleadingForm) -> bool {
    form.error = None;
    if form.remember_firm {
        if let Err(e) = app.run("law.firm.set", firm_json(&form.firm)) {
            form.error = Some(e);
            return false;
        }
        form.firm_saved = true;
        // The options page reads them again.
        app.law.firm_draft = None;
    }
    let old = app.session.document_id();
    match app.run("law.pleadings.new", form.params()) {
        Ok(_) => {
            place_caret(app);
            true
        }
        Err(e) if e.contains(NOT_BLANK) => match app.run("file.new", json!({})) {
            Ok(v) if v.get("pending").is_some() => {
                form.pending = Some(old);
                false
            }
            Ok(_) => create(app, form),
            Err(e) => {
                form.error = Some(e);
                false
            }
        },
        Err(e) => {
            form.error = Some(e);
            false
        }
    }
}

/// Write the pleading into the (new, blank) document. `true`: done.
fn create(app: &mut Ctx<'_>, form: &mut PleadingForm) -> bool {
    match app.run("law.pleadings.new", form.params()) {
        Ok(_) => {
            place_caret(app);
            true
        }
        Err(e) => {
            form.error = Some(e);
            false
        }
    }
}

/// The caret goes to the first placeholder (`[`) for the person to fill in.
fn place_caret(app: &mut Ctx<'_>) {
    if app.run("select.text", json!({"text": "["})).is_ok() {
        app.session.sel = wordcraft_engine::Selection::caret(app.session.sel.anchor.clone());
    }
    app.canvas.want_focus = true;
}

/// File › Options › LawCraft: the firm details New Pleading signs with.
pub(crate) fn firm_section(app: &mut Ctx<'_>, ui: &mut Ui) {
    ui.label(RichText::new("Firm details").font(semibold(14.0)));
    ui.label(RichText::new("New Pleading signs with these. They stay on this computer.").small().weak());
    let mut draft = match app.law.firm_draft.take() {
        Some(d) => d,
        None => firm_from(&app.run("law.firm.get", json!({})).unwrap_or_default()),
    };
    if firm_fields(ui, &mut draft, "law_options_firm") {
        app.law.firm_dirty = true;
        app.law.firm_message = None;
    }
    app.law.firm_draft = Some(draft);
    if ui.add_enabled(app.law.firm_dirty, egui::Button::new("Save firm details")).clicked() {
        save_firm(app);
    }
    if let Some(m) = &app.law.firm_message {
        ui.colored_label(ui.visuals().error_fg_color, m);
    }
}

/// Save the options page's firm details if they changed; `false` (with the reason shown) when
/// they can't be.
pub(crate) fn save_firm(app: &mut Ctx<'_>) -> bool {
    if !app.law.firm_dirty {
        return true;
    }
    let firm = app.law.firm_draft.as_ref().map(firm_json).unwrap_or_default();
    match app.run("law.firm.set", firm) {
        Ok(_) => {
            app.law.firm_dirty = false;
            app.law.firm_message = None;
            true
        }
        Err(e) => {
            app.law.firm_message = Some(e);
            false
        }
    }
}
