//! The New Pleading window (`law.pleadings.new` with no params asks for it), the same form as
//! Edit Pleading (`law.pleadings.edit` with no params), and what it shares with File › Options ›
//! LawCraft: the firm details and the saved courts. English only, like the options page.

use egui::{RichText, Ui};
use serde_json::{Map, Value, json};
use wordcraft_ui_egui::file_dialogs::FileDialogRequest;
use wordcraft_ui_egui::theme::semibold;

use crate::Ctx;

/// What Edit Pleading says beside what it can't change.
const STRUCTURE: &str = "Start a new pleading to change this.";

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

    /// A side from `law.pleadings.current`, its role picked from `roles` when it is one of them.
    fn from_json(v: &Value, roles: &[&str]) -> Self {
        let role = v["role"].as_str().unwrap_or("").to_string();
        let parties = v["names"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|p| PartyRow {
                        name: p["name"].as_str().unwrap_or("").to_string(),
                        client: p["client"].as_bool().unwrap_or(false),
                        pro_se: p["proSe"].as_bool().unwrap_or(false),
                        address: p["address"].as_str().unwrap_or("").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { parties, other: !roles.contains(&role.as_str()), role }
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
    /// The caption's division line; empty for none.
    pub division: String,
    pub case_number: String,
    pub first: SideForm,
    pub second: SideForm,
    /// `YYYY-MM-DD`, prefilled with today so the person sees it and can correct it.
    pub filing_date: String,
    /// Petition only.
    pub verified: bool,
    /// Edit mode, a pleading served by USPS on a party not in the caption: their name and
    /// address (`servedParty`, `servedAddress`).
    pub served: Option<(String, String)>,
    pub firm: Firm,
    pub remember_firm: bool,
    /// Firm details were saved already (their section starts closed).
    pub firm_saved: bool,
    /// From contacts… was pressed on this row: ask for the file once the window is drawn.
    pub contact_ask: Option<(u8, usize)>,
    /// Why the last Create didn't go through.
    pub error: Option<String>,
    /// File › New is waiting on its Save Changes prompt: the document that was open then.
    pub pending: Option<u64>,
    /// The saved firm details have been read in.
    loaded: bool,
    /// Edit Pleading: the form of the open pleading, changing its text only.
    pub edit: bool,
    /// Edit mode: the pleading has a division line (whether it has one can't change).
    pub had_division: bool,
    /// The saved courts, read when the window opens.
    pub courts: Vec<Value>,
    /// Save the form's court under [`Self::court_name`] on Create.
    pub save_court: bool,
    pub court_name: String,
    /// A party row whose From contacts… file is being picked: (side 1 or 2, row).
    pub contact_for: Option<(u8, usize)>,
    /// The contacts in the picked file, when it has more than one: pick one.
    pub contact_choices: Vec<Value>,
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
            served: None,
            firm: Firm::default(),
            remember_firm: true,
            firm_saved: false,
            error: None,
            contact_ask: None,
            pending: None,
            loaded: false,
            division: String::new(),
            edit: false,
            had_division: false,
            courts: Vec::new(),
            save_court: false,
            court_name: String::new(),
            contact_for: None,
            contact_choices: Vec::new(),
        }
    }
}

impl PleadingForm {
    /// Edit Pleading's form: the open pleading's values (`law.pleadings.current`).
    pub fn from_current(v: &Value) -> Self {
        let kind = TYPES.iter().map(|(k, _, _)| *k).find(|k| v["type"] == *k).unwrap_or("motion");
        let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let division = text("division");
        Self {
            kind,
            title: text("title"),
            court_type: text("courtType"),
            county: text("county"),
            had_division: v.get("division").is_some(),
            division,
            case_number: text("caseNumber"),
            first: SideForm::from_json(&v["firstSide"], &FIRST_ROLES),
            second: SideForm::from_json(&v["secondSide"], &SECOND_ROLES),
            filing_date: v["filingDate"].as_str().map(str::to_string).unwrap_or_else(today),
            verified: v["verified"].as_bool().unwrap_or(false),
            edit: true,
            // The firm block isn't part of Edit Pleading.
            remember_firm: false,
            firm_saved: true,
            served: v.get("servedParty").map(|_| (text("servedParty"), text("servedAddress"))),
            ..Self::default()
        }
    }

    /// The name Save this court offers: "Adair County Circuit".
    pub fn default_court_name(&self) -> String {
        let tc = |t: &str| wordcraft_law::pleadings::title_case(t.trim());
        format!("{} County {}", tc(&self.county), tc(&self.court_type)).trim().to_string()
    }

    /// Fill the court fields from a saved court.
    pub fn use_court(&mut self, court: &Value) {
        let text = |k: &str| court[k].as_str().unwrap_or("").to_string();
        self.court_type = text("courtType");
        self.county = text("county");
        if !self.edit || self.had_division {
            self.division = text("division");
        }
    }

    /// Fill a party row from a contact: its name (or organization) and mailing address.
    pub fn use_contact(&mut self, (side, row): (u8, usize), contact: &Value) {
        let side = if side == 1 { &mut self.first } else { &mut self.second };
        let Some(p) = side.parties.get_mut(row) else { return };
        let name = contact["name"].as_str().filter(|n| !n.trim().is_empty()).or(contact["organization"].as_str()).unwrap_or("");
        if !name.trim().is_empty() {
            p.name = name.trim().to_string();
        }
        if let Some(a) = contact["address"].as_str().filter(|a| !a.trim().is_empty()) {
            p.address = a.trim().to_string();
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
        if self.save_court && self.court_name.trim().is_empty() {
            return fill("the court's name");
        }
        if self.edit {
            return None;
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
        if !self.division.trim().is_empty() {
            p["division"] = json!(self.division.trim());
        }
        if self.save_court {
            p["saveCourt"] = json!(self.court_name.trim());
        }
        if self.edit {
            // Edit Pleading leaves the signature block alone.
            if let Some(o) = p.as_object_mut() {
                o.remove("firm");
            }
            if let Some((party, address)) = &self.served {
                p["service"] = json!("efileUsps");
                p["servedParty"] = json!(party);
                p["servedAddress"] = json!(address);
            }
        }
        p
    }
}

/// `law.pleadings.new` asked for the window.
pub(crate) fn open(law: &mut crate::LawUi, what: &str) {
    if law.pleading.is_some() {
        return;
    }
    match what {
        "newPleading" => law.pleading = Some(PleadingForm::default()),
        // Filled from the document when the window first draws (it needs the session).
        "editPleading" => law.pleading = Some(PleadingForm { edit: true, ..PleadingForm::default() }),
        _ => {}
    }
}

/// Draw the window while it is open, and finish a Create that waited on File › New.
pub(crate) fn show(app: &mut Ctx<'_>, ctx: &egui::Context) {
    let Some(mut form) = app.law.pleading.take() else { return };
    if !form.loaded {
        if form.edit {
            match app.run("law.pleadings.current", json!({})) {
                Ok(v) => form = PleadingForm::from_current(&v),
                Err(e) => {
                    app.status(e);
                    return;
                }
            }
        } else {
            let saved = app.run("law.firm.get", json!({})).unwrap_or_default();
            if saved.as_object().is_some_and(|o| !o.is_empty()) {
                form.firm = firm_from(&saved);
                form.firm_saved = true;
            }
        }
        form.loaded = true;
        form.courts = app.run("law.courts.list", json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    }
    if let Some(at) = form.contact_ask.take() {
        form.contact_for = Some(at);
        app.addin_ask_file(wordcraft_law::MODULE, FileDialogRequest::Open { purpose: "contacts".into() }, "contacts");
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
        let (heading, action) = if form.edit { ("Edit Pleading", "Update") } else { ("New Pleading", "Create") };
        egui::Window::new(heading).id(egui::Id::unique("law_new_pleading")).collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
            ui.set_min_width(420.0);
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| form_ui(ui, &mut form));
            if let Some(e) = &form.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            let missing = form.missing().map(|m| if form.edit { m.replace(" to create.", " to update.") } else { m });
            if let Some(why) = &missing {
                ui.label(RichText::new(why).small().weak());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(missing.is_none(), egui::Button::new(action)).clicked() && submit(app, &mut form) {
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

/// The New Pleading form's fields. In edit mode what decides the paragraphs (type, parties,
/// client and pro se marks) is greyed out.
fn form_ui(ui: &mut Ui, form: &mut PleadingForm) {
    let edit = form.edit;
    let field = |ui: &mut Ui, label: &str, value: &mut String, enabled: bool| {
        let l = ui.label(label);
        ui.add_enabled(enabled, egui::TextEdit::singleline(value).desired_width(260.0)).labelled_by(l.id);
        ui.end_row();
    };
    if !form.contact_choices.is_empty() {
        contact_choice_ui(ui, form);
        return;
    }
    egui::Grid::new("law_pleading_form").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label("Type");
        ui.horizontal(|ui| {
            for (kind, label, title) in TYPES {
                if ui.add_enabled_ui(!edit, |ui| ui.selectable_label(form.kind == kind, label)).inner.clicked() && form.kind != kind {
                    form.kind = kind;
                    form.title = title.to_string();
                }
            }
        });
        ui.end_row();
        if edit {
            ui.label("");
            ui.label(RichText::new(format!("Type and parties: {STRUCTURE}")).small().weak());
            ui.end_row();
        }
        field(ui, "Title", &mut form.title, true);
        if !form.courts.is_empty() {
            let l = ui.label("Saved court");
            let mut pick: Option<Value> = None;
            craft_ui::ComboBox::from_id_salt("law_saved_court")
                .selected_text("Choose a court…")
                .width(260.0)
                .show_ui(ui, |ui| {
                    for c in &form.courts {
                        if ui.selectable_label(false, c["name"].as_str().unwrap_or("")).clicked() {
                            pick = Some(c.clone());
                        }
                    }
                })
                .response
                .labelled_by(l.id);
            if let Some(c) = pick {
                form.use_court(&c);
            }
            ui.end_row();
        }
        field(ui, "Court type", &mut form.court_type, true);
        field(ui, "County", &mut form.county, true);
        field(ui, "Division", &mut form.division, !edit || form.had_division);
        field(ui, "Case No.", &mut form.case_number, true);
    });
    ui.horizontal(|ui| {
        if ui.checkbox(&mut form.save_court, "Save this court").changed() && form.save_court && form.court_name.trim().is_empty() {
            form.court_name = form.default_court_name();
        }
        if form.save_court {
            ui.add(egui::TextEdit::singleline(&mut form.court_name).hint_text("Name in the list").desired_width(200.0));
        }
    });
    let petition = form.kind == "petition";
    // Our clients are all on one side: ticking Client on one side clears the other's.
    let mut ask = None;
    if side_ui(ui, "First side", &FIRST_ROLES, &mut form.first, petition, edit, (1, &mut ask), "law_side_first") {
        form.second.parties.iter_mut().for_each(|p| p.client = false);
    }
    if side_ui(ui, "Second side", &SECOND_ROLES, &mut form.second, petition, edit, (2, &mut ask), "law_side_second") {
        form.first.parties.iter_mut().for_each(|p| p.client = false);
    }
    if ask.is_some() {
        form.contact_ask = ask;
    }
    ui.add_space(6.0);
    egui::Grid::new("law_pleading_form_end").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let l = ui.label("Filing date");
        ui.add(egui::TextEdit::singleline(&mut form.filing_date).hint_text("YYYY-MM-DD").desired_width(260.0)).labelled_by(l.id);
        ui.end_row();
        if petition {
            ui.label("");
            ui.add_enabled(!edit, egui::Checkbox::new(&mut form.verified, "Verified"));
            ui.end_row();
        }
    });
    if let Some((party, address)) = form.served.as_mut() {
        egui::Grid::new("law_pleading_served").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            let l = ui.label("Served party");
            ui.add(egui::TextEdit::singleline(party).desired_width(260.0)).labelled_by(l.id);
            ui.end_row();
            let l = ui.label("Served at");
            ui.add(egui::TextEdit::singleline(address).desired_width(260.0)).labelled_by(l.id);
            ui.end_row();
        });
    }
    if edit {
        return;
    }
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Firm details").default_open(!form.firm_saved).show(ui, |ui| {
        firm_fields(ui, &mut form.firm, "law_pleading_firm");
    });
    ui.checkbox(&mut form.remember_firm, "Remember firm details");
}

/// The picked contact file has several contacts: pick the one for the row.
fn contact_choice_ui(ui: &mut Ui, form: &mut PleadingForm) {
    ui.label(RichText::new("Choose a contact").font(semibold(13.0)));
    let mut chosen = None;
    egui::ScrollArea::vertical().max_height(360.0).id_salt("law_contacts").show(ui, |ui| {
        for (i, c) in form.contact_choices.iter().enumerate() {
            let name = c["name"].as_str().unwrap_or("");
            let org = c["organization"].as_str().filter(|o| !o.is_empty() && *o != name);
            let label = match org {
                Some(o) => format!("{name} ({o})"),
                None => name.to_string(),
            };
            if ui.selectable_label(false, label).clicked() {
                chosen = Some(i);
            }
        }
    });
    if let Some(i) = chosen {
        if let (Some(at), Some(c)) = (form.contact_for.take(), form.contact_choices.get(i).cloned()) {
            form.use_contact(at, &c);
        }
        form.contact_choices.clear();
    }
    if ui.button("Back").clicked() {
        form.contact_choices.clear();
        form.contact_for = None;
    }
}

/// One side: its role (from the list, or Other typed in) and its parties, each with Client and,
/// outside a petition, Pro se with a mailing address, and From contacts…. `true` when a Client
/// box was just ticked. In edit mode only the names, role and addresses change.
#[allow(clippy::too_many_arguments)]
fn side_ui(
    ui: &mut Ui,
    heading: &str,
    roles: &[&str],
    side: &mut SideForm,
    petition: bool,
    edit: bool,
    (n, ask): (u8, &mut Option<(u8, usize)>),
    salt: &str,
) -> bool {
    let mut ticked = false;
    ui.add_space(6.0);
    ui.label(RichText::new(heading).font(semibold(13.0)));
    ui.push_id(salt, |ui| {
        ui.horizontal(|ui| {
            let l = ui.label("Role");
            let shown = if side.other { "Other".to_string() } else { side.role.clone() };
            craft_ui::ComboBox::from_id_salt("role")
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
                    if ui.add_enabled(!edit, egui::Checkbox::new(&mut p.client, "Client")).changed() && p.client {
                        p.pro_se = false;
                        ticked = true;
                    }
                    if !petition {
                        ui.add_enabled(!p.client && !edit, egui::Checkbox::new(&mut p.pro_se, "Pro se"));
                    }
                    if ui.small_button("From contacts…").on_hover_text("Fill the name and mailing address from a contact file (.vcf)").clicked() {
                        *ask = Some((n, i));
                    }
                    if count > 1 && !edit && ui.small_button("Remove").clicked() {
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
        if side.parties.len() < MAX_NAMES && !edit && ui.small_button("Add name").clicked() {
            side.parties.push(PartyRow::default());
        }
    });
    ticked
}

/// A contact file was picked for a party row: one contact fills the row; several are offered
/// in the window to pick from.
pub(crate) fn contacts_picked(app: &mut Ctx<'_>, picked: Option<String>) -> Result<Value, String> {
    let Some(path) = picked else {
        if let Some(f) = app.law.pleading.as_mut() {
            f.contact_for = None;
        }
        return Ok(json!({"cancelled": true}));
    };
    let r = app.run("law.contacts.read", json!({"path": path}));
    let Some(form) = app.law.pleading.as_mut() else { return r };
    match &r {
        Ok(Value::Array(list)) if list.len() == 1 => {
            if let Some(at) = form.contact_for.take() {
                form.use_contact(at, &list[0]);
            }
        }
        Ok(Value::Array(list)) if list.is_empty() => {
            form.contact_for = None;
            form.error = Some("No contacts with a name in that file.".into());
        }
        Ok(Value::Array(list)) => form.contact_choices = list.clone(),
        Ok(_) => {}
        Err(e) => {
            form.contact_for = None;
            form.error = Some(e.clone());
        }
    }
    r
}

/// Create: save the firm details if asked, make sure the document is blank (File › New, with its
/// Save Changes prompt), then write the pleading. `true`: done, close the window.
fn submit(app: &mut Ctx<'_>, form: &mut PleadingForm) -> bool {
    form.error = None;
    if form.edit {
        return match app.run("law.pleadings.edit", form.params()) {
            Ok(_) => {
                app.canvas.want_focus = true;
                true
            }
            Err(e) => {
                form.error = Some(e);
                false
            }
        };
    }
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

/// File › Options › LawCraft: the saved courts New Pleading offers, with Rename and Delete.
pub(crate) fn courts_section(app: &mut Ctx<'_>, ui: &mut Ui) {
    ui.label(RichText::new("Courts").font(semibold(14.0)));
    ui.label(RichText::new("New Pleading offers these. Save one with Save this court in New Pleading.").small().weak());
    let courts = app.run("law.courts.list", json!({})).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    if courts.is_empty() {
        ui.label(RichText::new("No saved courts yet.").weak());
        return;
    }
    let mut act: Option<(&str, Value)> = None;
    egui::Grid::new("law_courts").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
        for c in &courts {
            let id = c["id"].as_str().unwrap_or("").to_string();
            match app.law.court_rename.as_mut().filter(|(r, _)| *r == id) {
                Some((_, name)) => {
                    ui.add(egui::TextEdit::singleline(name).desired_width(220.0));
                    if ui.button("Save").clicked() {
                        let mut v = c.clone();
                        v["name"] = json!(name.trim());
                        act = Some(("save", v));
                    }
                }
                None => {
                    let detail =
                        [c["courtType"].as_str(), c["county"].as_str(), c["division"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                    ui.label(c["name"].as_str().unwrap_or("")).on_hover_text(detail);
                    if ui.button("Rename").clicked() {
                        app.law.court_rename = Some((id.clone(), c["name"].as_str().unwrap_or("").to_string()));
                    }
                }
            }
            if ui.button("Delete").clicked() {
                act = Some(("delete", json!({"id": id})));
            }
            ui.end_row();
        }
    });
    if let Some((what, v)) = act {
        let r = if what == "save" { app.run("law.courts.save", v) } else { app.run("law.courts.delete", v) };
        app.law.court_rename = None;
        if let Err(e) = r {
            app.status(e);
        }
    }
}
