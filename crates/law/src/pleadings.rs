//! Pleadings: `law.pleadings.new` writes a Missouri circuit court filing skeleton (caption,
//! title, body, signature block, certificate of service or verification) into a blank document,
//! and `law.firm.get` / `law.firm.set` keep the firm details its signature block uses.
//!
//! Like applying a profile, nothing here edits the document tree: the skeleton is typed and
//! formatted through WordCraft's own commands (`text.insert`, `para.style`, `insert.table`,
//! `table.borders`…), all joined into the command's one undo step, so protection and Track
//! Changes treat it like typing. The layout comes from the `missouri-circuit-court` profile's
//! styles; the few per-paragraph settings a style can't carry (a caption line's indent, the space
//! before a signature line, list numbering) are set on the paragraph.
//!
//! The text is the firm's own pleading wording, in English: it is legal text, not interface.
//!
//! Every place a form value lands (a name, the county, the title…) goes into a plain-text
//! content control tagged `law:<field>[:<variant>]`, holding the value as typed in its title,
//! and the pleading's shape (type, which parties are clients or pro se, service) goes into the
//! `LawCraft pleading` custom property, with no names in it. `law.pleadings.edit` reads both back
//! to reopen the form, and rewrites every tagged control from the corrected values in one undo
//! step; [`field_text`] gives the text for a tag both times, so a name is cased the same way.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use wordcraft_doc::control::ControlRange;
use wordcraft_doc::{Document, Path, Pos, StoryRef};
use wordcraft_engine::cmd::{civil_from_days, now_unix, pos_json};
use wordcraft_engine::{CmdError, CmdResult, Session};

use crate::cmd::store;

/// The bundled profile the skeleton is formatted with.
pub const PROFILE: &str = "missouri-circuit-court";

/// What `law.pleadings.new` says when the open document isn't blank.
pub const NOT_BLANK: &str = "New Pleading writes into a blank document. Open one with File › New first.";

/// The pleading types, as `type` takes them.
pub const TYPES: [&str; 4] = ["motion", "notice", "answer", "petition"];

/// The firm block's fields, and whether a pleading needs each one.
pub const FIRM_FIELDS: [(&str, bool); 11] = [
    ("firmName", true),
    ("address1", true),
    ("address2", false),
    ("city", true),
    ("state", true),
    ("zip", true),
    ("phone", true),
    ("fax", false),
    ("email", true),
    ("attorneyName", true),
    ("barNumber", true),
];

/// Longest firm detail, in characters.
const MAX_FIRM_VALUE: usize = 200;
/// Longest other text parameter, in characters.
const MAX_TEXT: usize = 1000;
/// Most names on one side of the caption.
const MAX_NAMES: usize = 20;

/// The custom document property that marks a pleading New Pleading wrote, and holds its shape.
pub const PROPERTY: &str = "LawCraft pleading";
/// What Edit Pleading says about a document New Pleading didn't write.
pub const NOT_EDITABLE: &str = "This pleading was made before Edit Pleading. Use Find and Replace (Ctrl+H).";
/// What Edit Pleading says when the change needs different paragraphs, not different words.
pub const STRUCTURE: &str = "Start a new pleading to change this";
/// The tag prefix of LawCraft's content controls.
pub const TAG: &str = "law:";

/// Check a firm block: an object of the known fields only, each a string of at most 200
/// characters. Empty values are left out.
pub fn firm_fields(v: &Value) -> Result<BTreeMap<String, String>, String> {
    let Some(obj) = v.as_object() else { return Err("the firm details must be an object".into()) };
    let mut out = BTreeMap::new();
    for (key, value) in obj {
        if !FIRM_FIELDS.iter().any(|(k, _)| k == key) {
            let known: Vec<&str> = FIRM_FIELDS.iter().map(|(k, _)| *k).collect();
            return Err(format!("unknown firm detail `{key}` (known: {})", known.join(", ")));
        }
        let Some(text) = value.as_str() else { return Err(format!("`{key}` must be a string")) };
        if text.chars().count() > MAX_FIRM_VALUE {
            return Err(format!("`{key}` is longer than {MAX_FIRM_VALUE} characters"));
        }
        let text = text.trim();
        if !text.is_empty() {
            out.insert(key.clone(), text.to_string());
        }
    }
    Ok(out)
}

/// `law.firm.get`: the saved firm details, or `{}`.
pub fn firm_get(_: &mut Session, _: &Value) -> CmdResult {
    Ok(json!(store().firm()))
}

/// `law.firm.set`: check and save the firm details (they replace what was saved).
pub fn firm_set(_: &mut Session, v: &Value) -> CmdResult {
    let firm = firm_fields(v).map_err(CmdError::Params)?;
    if firm.is_empty() {
        return Err(CmdError::Params("no firm details given (firmName, address1, …)".into()));
    }
    store().set_firm(firm.clone()).map_err(|e| CmdError::Failed(e.to_string()))?;
    Ok(json!(firm))
}

/// `law.pleadings.new`: with no params, ask the interface for the New Pleading dialog; with
/// params, write the skeleton.
pub fn new_pleading(s: &mut Session, v: &Value) -> CmdResult {
    if v.is_null() || v.as_object().is_some_and(|o| o.is_empty()) {
        s.ui_requests.push(json!({"addin": crate::MODULE, "open": "newPleading"}));
        return Ok(json!({"pending": "newPleading"}));
    }
    let form = Form::read(v, true)?;
    // Before the profile goes on: its page-number footer isn't the person's text.
    if !is_blank(&s.doc) {
        return Err(CmdError::Params(NOT_BLANK.into()));
    }
    let Some(profile) = store().get(PROFILE).cloned() else { return Err(CmdError::Failed(format!("no court profile `{PROFILE}`"))) };
    crate::apply::apply(s, &profile, form.kind).map_err(|e| CmdError::Failed(e.to_string()))?;
    let sections = Writer { s, started: false, lists: Vec::new() }.write(&form)?;
    // File › Properties › Title: what a `TITLE` field in the footer shows (California's footer),
    // and the pleading's shape for Edit Pleading.
    s.join_next_undo();
    s.run("file.properties", &json!({"title": form.title, "custom": {PROPERTY: form.shape().to_string()}}))?;
    let mut out = json!({"profile": PROFILE, "type": form.kind, "sections": sections});
    save_court(v, &form, &mut out)?;
    Ok(out)
}

/// `saveCourt`: save the form's court under that name; the saved court goes in `out.court`.
fn save_court(v: &Value, form: &Form, out: &mut Value) -> Result<(), CmdError> {
    let Some(name) = text(v, "saveCourt")? else { return Ok(()) };
    let court = json!({"name": name, "courtType": form.court_type, "county": form.county, "division": form.division});
    let saved = crate::courts::save(&court).map_err(CmdError::Params)?;
    out["court"] = saved;
    Ok(())
}

/// Whether the document is a pleading New Pleading wrote (it has the [`PROPERTY`]).
pub fn is_pleading(doc: &Document) -> bool {
    shape_of(doc).is_some()
}

/// The [`PROPERTY`] of the document, parsed.
fn shape_of(doc: &Document) -> Option<Value> {
    let p = doc.custom_props.iter().find(|p| p.name.eq_ignore_ascii_case(PROPERTY))?;
    serde_json::from_str::<Value>(&p.value).ok().filter(|v| v.get("schema").and_then(Value::as_u64) == Some(1))
}

/// Disabled with [`NOT_EDITABLE`] on a document New Pleading didn't write.
pub fn edit_enabled(s: &Session) -> Option<&'static str> {
    if is_pleading(&s.doc) { None } else { Some(NOT_EDITABLE) }
}

/// LawCraft's tagged controls in the body: (field key with variant, control).
fn tagged(doc: &Document) -> Vec<(String, ControlRange)> {
    doc.control_ranges(StoryRef::Body).into_iter().filter_map(|r| Some((r.control.tag.strip_prefix(TAG)?.to_string(), r))).collect()
}

/// `law.pleadings.edit`: with no params, ask the interface for the form in edit mode; with
/// params (as `law.pleadings.new` takes them), rewrite every tagged control from them.
pub fn edit_pleading(s: &mut Session, v: &Value) -> CmdResult {
    let Some(shape) = shape_of(&s.doc) else { return Err(CmdError::Params(NOT_EDITABLE.into())) };
    if v.is_null() || v.as_object().is_some_and(|o| o.is_empty()) {
        s.ui_requests.push(json!({"addin": crate::MODULE, "open": "editPleading"}));
        return Ok(json!({"pending": "editPleading"}));
    }
    let form = Form::read(v, false)?;
    let new_shape = form.shape();
    if new_shape != shape {
        let what = ["type", "sides", "service", "verified", "division"].into_iter().find(|k| new_shape.get(*k) != shape.get(*k)).unwrap_or("type");
        let what = match what {
            "type" => "the pleading type",
            "sides" => "the parties' number, sides or client and pro se marks",
            "service" => "the service method",
            "verified" => "the verification",
            _ => "whether the caption has a division",
        };
        return Err(CmdError::Params(format!("{STRUCTURE}: Edit Pleading changes text, not {what}.")));
    }
    let mut controls = tagged(&s.doc);
    // Last first, so earlier offsets in the same paragraph stay put.
    controls.sort_by(|(_, a), (_, b)| (&b.start.path, b.start.off).cmp(&(&a.start.path, a.start.off)));
    let mut updated = 0;
    let mut seen = std::collections::BTreeSet::new();
    for (key, r) in &controls {
        let Some(want) = field_text(&form, key) else { continue };
        seen.insert(key.split_once(':').map_or(key.as_str(), |(b, _)| b).to_string());
        let raw = raw_value(&form, key).unwrap_or_default();
        let have = s.doc.control_text(r);
        if r.control.title != raw {
            s.join_next_undo();
            s.run("developer.properties", &json!({"at": pos_json(&r.content_start()), "title": raw}))?;
        }
        if have != want {
            s.join_next_undo();
            s.run("select.range", &json!({"anchor": pos_json(&r.content_start()), "focus": pos_json(&r.end)}))?;
            s.join_next_undo();
            s.run("text.insert", &json!({"text": want, "raw": true}))?;
            updated += 1;
        }
    }
    s.join_next_undo();
    s.run("file.properties", &json!({"title": form.title}))?;
    let missing: Vec<String> = form.keys().into_iter().filter(|k| !seen.contains(k)).collect();
    let mut out = json!({"updated": updated, "missing": missing});
    save_court(v, &form, &mut out)?;
    Ok(out)
}

/// `law.pleadings.current`: the form of the pleading in the document, as `law.pleadings.new`
/// takes it, rebuilt from its [`PROPERTY`] and its tagged controls (`firm` is left out).
pub fn current(s: &mut Session, _: &Value) -> CmdResult {
    let Some(shape) = shape_of(&s.doc) else { return Err(CmdError::Params(NOT_EDITABLE.into())) };
    // Each field's value: the text of its first as-typed control (what a person may have
    // corrected by hand), else the title of its first control (the value as typed).
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let controls = tagged(&s.doc);
    for (key, r) in &controls {
        let base = key.split_once(':').map_or(key.as_str(), |(b, _)| b);
        values.entry(base.to_string()).or_insert_with(|| r.control.title.clone());
    }
    let mut typed = std::collections::BTreeSet::new();
    for (key, r) in &controls {
        if key.contains(':') || key == "caseNumber" && r.control.title.is_empty() || !typed.insert(key.clone()) {
            continue;
        }
        let text = s.doc.control_text(r);
        if !text.is_empty() {
            values.insert(key.clone(), text);
        }
    }
    let get = |k: &str| values.get(k).cloned().unwrap_or_default();
    let kind = shape["type"].as_str().unwrap_or("motion").to_string();
    let mut sides = Vec::new();
    for (n, marks) in shape["sides"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let marks = marks.as_str().unwrap_or("");
        let names: Vec<Value> = marks
            .chars()
            .enumerate()
            .map(|(i, m)| {
                let mut o = json!({"name": get(&format!("s{}.name.{i}", n + 1)), "client": m == 'c', "proSe": m == 'p'});
                if m == 'p' {
                    o["address"] = json!(get(&format!("s{}.address.{i}", n + 1)));
                }
                o
            })
            .collect();
        sides.push(json!({"names": names, "role": get(&format!("s{}.role", n + 1))}));
    }
    let case = get("caseNumber");
    let mut out = json!({
        "type": kind,
        "title": get("title"),
        "courtType": get("courtType"),
        "county": get("county"),
        "caseNumber": if case.trim_matches('_').is_empty() { String::new() } else { case },
        "firstSide": sides.first().cloned().unwrap_or(Value::Null),
        "secondSide": sides.get(1).cloned().unwrap_or(Value::Null),
        "verified": shape["verified"].as_bool().unwrap_or(false),
    });
    if values.contains_key("division") {
        out["division"] = json!(get("division"));
    }
    if values.contains_key("date") {
        out["filingDate"] = json!(get("date"));
    }
    if kind != "petition" {
        out["service"] = shape["service"].clone();
        if shape["service"] == "efileUsps" {
            out["servedParty"] = json!(get("served.name"));
            out["servedAddress"] = json!(get("served.address"));
        }
    }
    Ok(out)
}

/// No text anywhere: one empty body paragraph, no comments, and nothing in the headers,
/// footers or notes.
fn is_blank(doc: &Document) -> bool {
    let body = match doc.body.as_slice() {
        [only] => only.as_para().is_some_and(|p| p.is_empty()),
        _ => false,
    };
    body && doc.comments.is_empty() && doc.parts.keys().all(|k| doc.plain_text(StoryRef::Part(*k)).trim().is_empty())
}

/// One party named in the caption, and how the pleading treats them.
struct Party {
    name: String,
    /// The firm represents this party.
    client: bool,
    /// Without counsel: served by USPS at `address`.
    pro_se: bool,
    address: Option<String>,
}

/// One side of the case: its parties and their role (Plaintiff, Respondent).
struct Side {
    parties: Vec<Party>,
    role: String,
}

impl Side {
    /// Whether the firm represents anyone on this side.
    fn has_client(&self) -> bool {
        self.parties.iter().any(|p| p.client)
    }
}

/// Someone served by USPS: the field keys of their name and address, and the values.
struct Mailed {
    name_key: String,
    addr_key: String,
    name: String,
    address: String,
}

/// The params, checked.
struct Form {
    kind: &'static str,
    title: String,
    court_type: String,
    county: String,
    /// The court's division, a caption line when set.
    division: String,
    case_number: String,
    first: Side,
    second: Side,
    /// The firm's clients are on the first side.
    client_first: bool,
    /// The parties served by USPS.
    mail: Vec<Mailed>,
    /// `service` is efileUsps.
    usps: bool,
    /// Some party the firm doesn't represent has counsel, served through e-filing.
    counsel: bool,
    date: (i64, u32, u32),
    verified: bool,
    firm: BTreeMap<String, String>,
}

impl Form {
    /// Read and check the command's params; the first problem is the error. `need_firm`: the
    /// firm details are required (New Pleading signs with them; Edit Pleading leaves them be).
    fn read(v: &Value, need_firm: bool) -> Result<Form, CmdError> {
        let kind_name = req(v, "type")?;
        let Some(kind) = TYPES.iter().copied().find(|t| *t == kind_name) else {
            return Err(CmdError::Params(format!("`type` must be one of {}", TYPES.join(", "))));
        };
        let petition = kind == "petition";
        let mut first = side(v, "firstSide")?;
        let mut second = side(v, "secondSide")?;
        let client_first = match (first.has_client(), second.has_client()) {
            (true, true) => return Err(CmdError::Params("the parties we represent must all be on one side (`client` is set on both)".into())),
            (true, false) => true,
            (false, true) => false,
            // Older callers name the side instead: everyone on it who isn't pro se.
            (false, false) => {
                let first_side = match text(v, "weRepresent")?.as_deref() {
                    Some("first") => true,
                    Some("second") => false,
                    Some(_) => return Err(CmdError::Params("`weRepresent` must be first or second".into())),
                    None => return Err(CmdError::Params("mark the parties we represent with `\"client\": true` (or give `weRepresent`)".into())),
                };
                let side = if first_side { &mut first } else { &mut second };
                for p in side.parties.iter_mut().filter(|p| !p.pro_se) {
                    p.client = true;
                }
                if !side.has_client() {
                    return Err(CmdError::Params("`weRepresent` names a side with no party we can represent".into()));
                }
                first_side
            }
        };
        let mut mail = Vec::new();
        let mut usps = false;
        if !petition {
            for (n, side) in [&first, &second].into_iter().enumerate() {
                for (i, p) in side.parties.iter().enumerate().filter(|(_, p)| p.pro_se) {
                    let Some(address) = &p.address else {
                        return Err(CmdError::Params(format!("`address` is required for pro se party {}", p.name)));
                    };
                    let (name_key, addr_key) = (format!("s{}.name.{i}", n + 1), format!("s{}.address.{i}", n + 1));
                    mail.push(Mailed { name_key, addr_key, name: p.name.clone(), address: address.clone() });
                }
            }
            match text(v, "service")?.as_deref() {
                None | Some("efileCounsel") => {}
                Some("efileUsps") => {
                    usps = true;
                    let (name, address) = (req(v, "servedParty")?, req(v, "servedAddress")?);
                    mail.push(Mailed { name_key: "served.name".into(), addr_key: "served.address".into(), name, address });
                }
                Some(_) => return Err(CmdError::Params("`service` must be efileCounsel or efileUsps".into())),
            }
        }
        let counsel = first.parties.iter().chain(&second.parties).any(|p| !p.client && !p.pro_se);
        let date = match text(v, "filingDate")? {
            Some(d) => parse_date(&d).ok_or_else(|| CmdError::Params(format!("`filingDate` must be a date as YYYY-MM-DD, not `{d}`")))?,
            // ponytail: UTC "today", as insert.dateTime does; nothing here knows the local zone.
            None => civil_from_days((now_unix() / 86_400) as i64),
        };
        let verified = match v.get("verified") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(CmdError::Params("`verified` must be true or false".into())),
        };
        let firm = match v.get("firm") {
            Some(f) if !f.is_null() => firm_fields(f).map_err(|e| CmdError::Params(format!("`firm`: {e}")))?,
            _ => store().firm().clone(),
        };
        if need_firm {
            if firm.is_empty() {
                return Err(CmdError::Params("`firm` is required: no firm details are saved (law.firm.set)".into()));
            }
            if let Some((key, _)) = FIRM_FIELDS.iter().find(|(k, needed)| *needed && !firm.contains_key(*k)) {
                return Err(CmdError::Params(format!("`firm.{key}` is required")));
            }
        }
        // A saved court fills the court fields; ones given as well win.
        let court = match text(v, "court")? {
            Some(id) => Some(crate::courts::get(&id).ok_or_else(|| CmdError::Params(format!("no saved court `{id}` (law.courts.list)")))?),
            None => None,
        };
        let from_court = |key: &str| court.as_ref().and_then(|c| c.get(key)).and_then(Value::as_str).map(str::to_string).filter(|t| !t.is_empty());
        let county = match text(v, "county")?.or_else(|| from_court("county")) {
            Some(c) => c,
            None => return Err(CmdError::Params("`county` is required".into())),
        };
        Ok(Form {
            kind,
            title: req(v, "title")?,
            court_type: text(v, "courtType")?.or_else(|| from_court("courtType")).unwrap_or_else(|| "CIRCUIT".into()),
            county,
            division: text(v, "division")?.or_else(|| from_court("division")).unwrap_or_default(),
            // A petition may be filed before a case number is assigned.
            case_number: if petition { text(v, "caseNumber")?.unwrap_or_default() } else { req(v, "caseNumber")? },
            first,
            second,
            client_first,
            mail,
            usps,
            counsel,
            date,
            verified,
            firm,
        })
    }

    /// The side the firm's clients are on.
    fn client(&self) -> &Side {
        if self.client_first { &self.first } else { &self.second }
    }

    /// The names of the parties the firm represents.
    fn clients(&self) -> Vec<String> {
        self.client().parties.iter().filter(|p| p.client).map(|p| p.name.clone()).collect()
    }

    /// A firm detail by field name, empty when it isn't set.
    fn firm(&self, key: &str) -> &str {
        self.firm.get(key).map(String::as_str).unwrap_or("")
    }

    /// The side `n` (1 or 2).
    fn side(&self, n: &str) -> Option<&Side> {
        match n {
            "s1" => Some(&self.first),
            "s2" => Some(&self.second),
            _ => None,
        }
    }

    /// The pleading's shape, kept in the [`PROPERTY`]: no names, only what decides which
    /// paragraphs there are. A side is a letter per party: `c` client, `p` pro se, `n` neither.
    fn shape(&self) -> Value {
        let marks = |s: &Side| {
            s.parties
                .iter()
                .map(|p| {
                    if p.client {
                        'c'
                    } else if p.pro_se {
                        'p'
                    } else {
                        'n'
                    }
                })
                .collect::<String>()
        };
        let service = if self.kind == "petition" {
            "none"
        } else if self.usps {
            "efileUsps"
        } else {
            "efileCounsel"
        };
        json!({
            "schema": 1,
            "type": self.kind,
            "sides": [marks(&self.first), marks(&self.second)],
            "service": service,
            "verified": self.kind == "petition" && self.verified,
            "division": !self.division.is_empty(),
        })
    }

    /// Every field key (without variant) this form has a value for.
    fn keys(&self) -> Vec<String> {
        let mut out: Vec<String> = ["title", "courtType", "county", "caseNumber", "s1.role", "s2.role"].iter().map(|k| k.to_string()).collect();
        if !self.division.is_empty() {
            out.push("division".into());
        }
        for (n, side) in [("s1", &self.first), ("s2", &self.second)] {
            for i in 0..side.parties.len() {
                out.push(format!("{n}.name.{i}"));
            }
        }
        for m in &self.mail {
            out.push(m.addr_key.clone());
            if m.name_key == "served.name" {
                out.push(m.name_key.clone());
            }
        }
        if self.kind != "petition" {
            out.push("date".into());
        }
        out
    }
}

/// A field's value as typed, by key (`county`, `s1.name.0`; a variant suffix is ignored):
/// what a tagged control keeps in its title.
fn raw_value(f: &Form, key: &str) -> Option<String> {
    let base = key.split_once(':').map_or(key, |(b, _)| b);
    let parts: Vec<&str> = base.split('.').collect();
    Some(match parts.as_slice() {
        ["title"] => f.title.clone(),
        ["courtType"] => f.court_type.clone(),
        ["county"] => f.county.clone(),
        ["division"] => f.division.clone(),
        ["caseNumber"] => f.case_number.clone(),
        ["date"] => format!("{:04}-{:02}-{:02}", f.date.0, f.date.1, f.date.2),
        [s, "role"] => f.side(s)?.role.clone(),
        [s, "name", i] => f.side(s)?.parties.get(i.parse::<usize>().ok()?)?.name.clone(),
        [s, "address", i] => f.side(s)?.parties.get(i.parse::<usize>().ok()?)?.address.clone()?,
        ["served", "name"] => f.mail.iter().find(|m| m.name_key == "served.name")?.name.clone(),
        ["served", "address"] => f.mail.iter().find(|m| m.addr_key == "served.address")?.address.clone(),
        _ => return None,
    })
}

/// The text a tagged control holds for a form: the value as typed, or a variant of it — `up`
/// upper case, `tc` title case, `long` a long date, `side` / `clients` a role made plural for
/// the side's parties or for the clients. `None` for a key the form has no value for.
fn field_text(f: &Form, key: &str) -> Option<String> {
    let raw = raw_value(f, key)?;
    let (base, variant) = key.split_once(':').unwrap_or((key, ""));
    Some(match variant {
        "" if base == "caseNumber" && raw.is_empty() => "____________".into(),
        "" => raw,
        "up" => raw.to_uppercase(),
        "tc" => title_case(&raw),
        "long" => long_date(f.date),
        "side" => plural_role(&raw, f.side(base.split('.').next()?)?.parties.len()),
        "clients" => plural_role(&raw, f.clients().len()),
        _ => return None,
    })
}

/// A text param: `None` when missing, null or empty.
fn text(v: &Value, key: &str) -> Result<Option<String>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if t.chars().count() > MAX_TEXT => Err(CmdError::Params(format!("`{key}` is longer than {MAX_TEXT} characters"))),
        Some(Value::String(t)) => Ok(Some(t.trim().to_string()).filter(|t| !t.is_empty())),
        Some(_) => Err(CmdError::Params(format!("`{key}` must be a string"))),
    }
}

/// A required text param.
fn req(v: &Value, key: &str) -> Result<String, CmdError> {
    text(v, key)?.ok_or_else(|| CmdError::Params(format!("`{key}` is required")))
}

/// A true/false flag on a party; missing is false.
fn flag(o: &Value, key: &str, side: &str) -> Result<bool, CmdError> {
    match o.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(CmdError::Params(format!("`{side}.names[].{key}` must be true or false"))),
    }
}

/// A party side param, checked: `{"names": [name], "role": string}`, each name a string or
/// `{"name": string, "client"?: bool, "proSe"?: bool, "address"?: string}`.
fn side(v: &Value, key: &str) -> Result<Side, CmdError> {
    let Some(obj) = v.get(key).filter(|o| o.is_object()) else {
        return Err(CmdError::Params(format!("`{key}` is required: {{\"names\": [string], \"role\": string}}")));
    };
    let Some(list) = obj.get("names").and_then(Value::as_array) else { return Err(CmdError::Params(format!("`{key}.names` is required"))) };
    if list.len() > MAX_NAMES {
        return Err(CmdError::Params(format!("`{key}.names` has more than {MAX_NAMES} names")));
    }
    let mut parties = Vec::new();
    for n in list {
        let (name, client, pro_se, address) = match n {
            Value::String(name) => (name.as_str(), false, false, None),
            Value::Object(o) => {
                let Some(name) = o.get("name").and_then(Value::as_str) else {
                    return Err(CmdError::Params(format!("`{key}.names` entries need a `name` string")));
                };
                let address = text(n, "address")
                    .map_err(|_| CmdError::Params(format!("`{key}.names[].address` must be a string of at most {MAX_TEXT} characters")))?;
                (name, flag(n, "client", key)?, flag(n, "proSe", key)?, address)
            }
            _ => return Err(CmdError::Params(format!("`{key}.names` must be strings or {{\"name\": string, …}}"))),
        };
        if name.chars().count() > MAX_TEXT {
            return Err(CmdError::Params(format!("`{key}.names` has a name longer than {MAX_TEXT} characters")));
        }
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if client && pro_se {
            return Err(CmdError::Params(format!("`{key}`: {name} can't be both our client and pro se")));
        }
        // An address typed on several lines reads as one.
        let address = address.map(|a| a.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(", "));
        parties.push(Party { name: name.to_string(), client, pro_se, address });
    }
    if parties.is_empty() {
        return Err(CmdError::Params(format!("`{key}.names` is required")));
    }
    let role = text(obj, "role").map_err(|_| CmdError::Params(format!("`{key}.role` must be a string")))?;
    let role = role.ok_or_else(|| CmdError::Params(format!("`{key}.role` is required")))?;
    Ok(Side { parties, role })
}

/// `YYYY-MM-DD`, a real date.
fn parse_date(d: &str) -> Option<(i64, u32, u32)> {
    let mut it = d.split('-');
    let (y, m, day) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    let (y, m, day): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, day.parse().ok()?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (1..=days).contains(&day).then_some((y, m, day))
}

/// `15th day of October, 2026`.
pub fn long_date((y, m, d): (i64, u32, u32)) -> String {
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let month = MONTHS.get((m as usize).saturating_sub(1)).copied().unwrap_or("");
    format!("{d}{} day of {month}, {y}", ordinal_suffix(d))
}

/// The English ordinal suffix for a day of the month: st, nd, rd or th.
fn ordinal_suffix(d: u32) -> &'static str {
    match (d % 10, d % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// The role for `n` parties: the usual roles take an s ("Petitioners"); a role typed in for
/// Other is left as typed.
pub fn plural_role(role: &str, n: usize) -> String {
    const ROLES: [&str; 4] = ["Petitioner", "Plaintiff", "Respondent", "Defendant"];
    if n > 1 && ROLES.iter().any(|r| r.eq_ignore_ascii_case(role)) {
        let s = if role.chars().any(char::is_lowercase) { "s" } else { "S" };
        format!("{role}{s}")
    } else {
        role.to_string()
    }
}

/// "A", "A and B", "A, B, and C".
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// "CIRCUIT" → "Circuit", "PETITION FOR DISSOLUTION OF MARRIAGE" → "Petition for Dissolution of
/// Marriage": small words stay lower case after the first.
pub fn title_case(s: &str) -> String {
    const SMALL: [&str; 15] = ["a", "an", "and", "as", "at", "but", "by", "for", "in", "nor", "of", "on", "or", "the", "to"];
    s.split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            let lower = w.to_lowercase();
            if i > 0 && SMALL.contains(&lower.as_str()) {
                return lower;
            }
            let mut chars = lower.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// A paragraph's text with the spans that hold form values: each becomes a tagged control.
#[derive(Default)]
struct Txt {
    text: String,
    /// (field key with variant, value as typed, start, end) in bytes of `text`.
    spans: Vec<(String, String, usize, usize)>,
}

impl From<&str> for Txt {
    fn from(t: &str) -> Self {
        Txt { text: t.to_string(), spans: Vec::new() }
    }
}

impl From<String> for Txt {
    fn from(text: String) -> Self {
        Txt { text, spans: Vec::new() }
    }
}

impl Txt {
    /// Literal text.
    fn lit(mut self, t: &str) -> Self {
        self.text.push_str(t);
        self
    }

    /// A form value by field key ([`field_text`]); nothing when the form has none.
    fn field(mut self, f: &Form, key: &str) -> Self {
        if let Some(t) = field_text(f, key).filter(|t| !t.is_empty()) {
            let start = self.text.len();
            self.text.push_str(&t);
            self.spans.push((key.to_string(), raw_value(f, key).unwrap_or_default(), start, self.text.len()));
        }
        self
    }

    /// `other` after this text.
    fn join(mut self, other: Txt) -> Self {
        let at = self.text.len();
        self.text.push_str(&other.text);
        self.spans.extend(other.spans.into_iter().map(|(k, r, a, b)| (k, r, a + at, b + at)));
        self
    }

    /// Names joined as [`join_names`] does, each one a field.
    fn names(mut self, f: &Form, keys: &[String]) -> Self {
        let n = keys.len();
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                self = self.lit(match (n, i + 1 == n) {
                    (2, _) => " and ",
                    (_, true) => ", and ",
                    _ => ", ",
                });
            }
            self = self.field(f, k);
        }
        self
    }
}

/// The field keys (as typed) of the parties the firm represents.
fn client_keys(f: &Form) -> Vec<String> {
    let n = if f.client_first { "s1" } else { "s2" };
    f.client().parties.iter().enumerate().filter(|(_, p)| p.client).map(|(i, _)| format!("{n}.name.{i}")).collect()
}

/// The field key of the clients' role, plural for more than one client.
fn client_role_key(f: &Form) -> &'static str {
    if f.client_first { "s1.role:clients" } else { "s2.role:clients" }
}

/// One line of a block that stays on one page: its text and the space before and after it.
struct Line {
    style: &'static str,
    text: Txt,
    before: Option<f32>,
    after: Option<f32>,
    bold: bool,
}

/// A plain line of `text` in `style`, with no extra spacing and not bold.
fn line(style: &'static str, text: impl Into<Txt>) -> Line {
    Line { style, text: text.into(), before: None, after: None, bold: false }
}

/// Types the skeleton through WordCraft's commands, each joined to the outer undo step.
struct Writer<'a> {
    s: &'a mut Session,
    /// Whether the caret's paragraph already holds something (the next one needs a new paragraph).
    started: bool,
    /// Paragraphs to number once the text is in, with their list kind.
    lists: Vec<(Pos, &'static str)>,
}

impl Writer<'_> {
    /// Run a command as part of the outer undo step.
    fn run(&mut self, id: &str, params: Value) -> CmdResult {
        self.s.join_next_undo();
        self.s.run(id, &params)
    }

    /// A new paragraph in `style` holding `text`, its form values in tagged controls. A
    /// paragraph made by splitting the last one starts without its direct formatting.
    fn para(&mut self, style: &str, text: impl Into<Txt>) -> Result<(), CmdError> {
        let t: Txt = text.into();
        if self.started {
            self.run("text.insert", json!({"text": "\r", "raw": true}))?;
            self.run("para.reset", json!({}))?;
        }
        self.started = true;
        self.run("para.style", json!({"style": style}))?;
        if t.text.is_empty() {
            return Ok(());
        }
        self.run("text.insert", json!({"text": t.text, "raw": true}))?;
        if t.spans.is_empty() {
            return Ok(());
        }
        let end = self.s.sel.focus.clone();
        let base = end.off.saturating_sub(t.text.len());
        // Last first: wrapping one span moves only what comes after it.
        for (key, raw, a, b) in t.spans.iter().rev() {
            let at = |off: usize| pos_json(&Pos { off: base + off, ..end.clone() });
            self.run("select.range", json!({"anchor": at(*a), "focus": at(*b)}))?;
            self.run("developer.plainText", json!({"tag": format!("{TAG}{key}"), "title": raw}))?;
            self.run("developer.properties", json!({"lockDelete": true}))?;
        }
        let len = self.s.doc.para_at(&end).map(|p| p.len()).unwrap_or(end.off);
        self.run("caret.set", json!({"pos": pos_json(&Pos { off: len, ..end })}))?;
        Ok(())
    }

    /// Bold the first `len` bytes of the caret's paragraph (the lead words), caret back at its end.
    fn bold_lead(&mut self, len: usize) -> Result<(), CmdError> {
        let end = self.s.sel.focus.clone();
        let start = Pos { off: 0, ..end.clone() };
        let lead = Pos { off: len, ..end.clone() };
        self.run("select.range", json!({"anchor": pos_json(&start), "focus": pos_json(&lead)}))?;
        self.run("format.bold", json!({"value": true}))?;
        self.run("caret.set", json!({"pos": pos_json(&end)}))?;
        Ok(())
    }

    /// A Body paragraph whose lead words are bold.
    fn lead(&mut self, lead: &str, rest: Txt) -> Result<(), CmdError> {
        self.para("Body", Txt::from(lead).join(rest))?;
        self.bold_lead(lead.len())
    }

    /// A paragraph numbered in a list (`numbered` 1., `lowerLetterDot` a.), numbered at the end.
    fn listed(&mut self, kind: &'static str, text: &str) -> Result<(), CmdError> {
        self.para("Numbered", text)?;
        self.lists.push((self.s.sel.focus.clone(), kind));
        Ok(())
    }

    /// Lines kept together on one page.
    fn block(&mut self, lines: Vec<Line>) -> Result<(), CmdError> {
        let n = lines.len();
        for (i, l) in lines.into_iter().enumerate() {
            let len = l.text.text.len();
            self.para(l.style, l.text)?;
            if l.bold {
                self.bold_lead(len)?;
            }
            if l.before.is_some() || l.after.is_some() {
                self.run("para.spacing", json!({"before": l.before, "after": l.after}))?;
            }
            self.run("para.keepLines", json!({"value": true}))?;
            if i + 1 < n {
                self.run("para.keepNext", json!({"value": true}))?;
            }
        }
        Ok(())
    }

    /// Write the whole pleading in order; returns the names of the sections written.
    fn write(mut self, f: &Form) -> Result<Vec<&'static str>, CmdError> {
        let mut sections = Vec::new();
        self.caption(f)?;
        sections.push("caption");
        self.para("Title", Txt::default().field(f, "title"))?;
        sections.push("title");
        self.body(f)?;
        sections.push("body");
        self.signature(f)?;
        sections.push("signature");
        if f.kind == "petition" {
            if f.verified {
                self.verification(f)?;
                sections.push("verification");
            }
        } else {
            self.certificate(f)?;
            sections.push("certificateOfService");
        }
        // Numbering last, so no paragraph typed after a numbered one inherits its list.
        for (at, kind) in std::mem::take(&mut self.lists) {
            self.run("caret.set", json!({"pos": pos_json(&at)}))?;
            self.run("para.numbering", json!({"kind": kind}))?;
            // The list's own indents would win over the style's: text at 72 pt (pleading.css: the
            // list's 0.75in padding plus the item's 0.25in), the number hanging just left of 54 pt
            // as the browser draws it (starting at 42 pt in the firm's PDF).
            self.run("para.indents", json!({"left": 72, "firstLine": -30}))?;
        }
        Ok(sections)
    }

    /// The court heading and the two-column caption table (parties on the left, case number on the right).
    fn caption(&mut self, f: &Form) -> Result<(), CmdError> {
        self.para("Court", Txt::from("IN THE ").field(f, "courtType").lit(" COURT OF ").field(f, "county").lit(" COUNTY"))?;
        self.para("Court State", "STATE OF MISSOURI")?;
        // The table goes in before this paragraph, which stays as the empty one after it.
        self.para("Normal", "")?;
        let table = self.run("insert.table", json!({"rows": 1, "cols": 2}))?;
        let table: Vec<u32> = table["table"].as_array().map(|a| a.iter().filter_map(|n| n.as_u64()?.try_into().ok()).collect()).unwrap_or_default();
        let Some(&at) = table.last() else { return Err(CmdError::Failed("insert.table did not say where the table went".into())) };
        let cell = |col: u32| {
            let mut path = table.clone();
            path.extend([0, col, 0]);
            Pos { story: StoryRef::Body, path: Path(path), off: 0 }
        };
        // One rule, between the two cells (the right edge of the parties' cell), and no other.
        self.run("table.borders", json!({"kind": "inside", "width": 1.5}))?;
        let width = self.s.doc.sections().first().map(|(_, sp)| sp.text_width()).unwrap_or(468.0);
        // Parties: 62% of the text width, 8 pt above and below, 20 pt before the rule.
        self.cell(&cell(0), width * 0.62, [8.0, 0.0, 8.0, 20.0])?;
        for (n, side, end) in [("s1", &f.first, ","), ("s2", &f.second, ".")] {
            for i in 0..side.parties.len() {
                self.para("Caption", Txt::default().field(f, &format!("{n}.name.{i}:up")))?;
            }
            self.para("Caption", Txt::default().field(f, &format!("{n}.role:side")).lit(end))?;
            self.run("para.indents", json!({"left": 54}))?;
            if end == "," {
                self.para("Caption", "v.")?;
                self.run("para.indents", json!({"left": 18}))?;
                self.run("para.spacing", json!({"before": 8, "after": 8}))?;
            }
        }
        // Case number: 38%, 20 pt after the rule; the division under it when there is one.
        self.cell(&cell(1), width * 0.38, [8.0, 20.0, 8.0, 0.0])?;
        self.para("Caption", Txt::from("Case No. ").field(f, "caseNumber"))?;
        if !f.division.is_empty() {
            self.para("Caption", Txt::from("Division: ").field(f, "division"))?;
        }
        // The empty Normal paragraph after the table.
        let mut after = table.clone();
        if let Some(last) = after.last_mut() {
            *last = at.saturating_add(1);
        }
        self.run("caret.set", json!({"pos": pos_json(&Pos { story: StoryRef::Body, path: Path(after), off: 0 })}))?;
        Ok(())
    }

    /// Put the caret in an empty cell and lay it out: width, vertical centring, padding.
    fn cell(&mut self, at: &Pos, width: f32, [top, left, bottom, right]: [f32; 4]) -> Result<(), CmdError> {
        self.run("caret.set", json!({"pos": pos_json(at)}))?;
        self.started = false;
        self.run("table.columnWidth", json!({"width": width}))?;
        self.run("table.properties", json!({"valign": "center"}))?;
        self.run("table.cellMargins", json!({"top": top, "left": left, "bottom": bottom, "right": right, "cell": true}))?;
        Ok(())
    }

    /// The opening paragraphs for the pleading's type, with placeholders for what the attorney fills in.
    fn body(&mut self, f: &Form) -> Result<(), CmdError> {
        let clients = client_keys(f);
        let role = client_role_key(f);
        let prays = if clients.len() > 1 { "pray" } else { "prays" };
        // " Jane Doe, Petitioner, by and through counsel"
        let comes = || Txt::from(" ").names(f, &clients).lit(", ").field(f, role).lit(", by and through counsel, ");
        let wherefore = || Txt::from(", ").field(f, role).lit(&format!(" {prays} "));
        match f.kind {
            "notice" => self.lead(
                "PLEASE TAKE NOTICE",
                Txt::from(
                    " that [motion] will be called up for hearing on [date] at [time], or as soon thereafter as counsel may be heard, before the ",
                )
                .field(f, "courtType:tc")
                .lit(" Court of ")
                .field(f, "county:tc")
                .lit(" County, Missouri."),
            ),
            "answer" => {
                self.lead("COMES NOW", comes().lit("and for Answer to the Petition states as follows:"))?;
                for n in 1..=3 {
                    self.listed("numbered", &format!("[Response to paragraph {n}.]"))?;
                }
                self.lead(
                    "WHEREFORE",
                    wherefore().lit("this Court deny the relief requested in the Petition and grant such other and further relief as the Court deems just and proper."),
                )
            }
            "petition" => {
                self.lead("COMES NOW", comes().lit("and for ").field(f, "title:tc").lit(" states as follows:"))?;
                for _ in 0..3 {
                    self.listed("numbered", "[Allegation.]")?;
                }
                self.lead("WHEREFORE", wherefore().lit("that this Court:"))?;
                self.listed("lowerLetterDot", "[Relief requested.]")?;
                self.listed("lowerLetterDot", "Grant such other and further relief as the Court deems just and proper.")
            }
            _ => {
                self.lead("COMES NOW", comes().lit("and in support of this motion states as follows:"))?;
                for _ in 0..3 {
                    self.listed("numbered", "[Grounds.]")?;
                }
                self.lead(
                    "WHEREFORE",
                    wherefore()
                        .lit("this Court [state the relief requested] and for such other and further relief as the Court deems just and proper."),
                )
            }
        }
    }

    /// The signature block from the saved firm details.
    fn signature(&mut self, f: &Form) -> Result<(), CmdError> {
        let sig = |text: Txt| line("Signature", text);
        let firm = |k: &str| Txt::from(f.firm(k));
        let mut lines = vec![sig("Respectfully Submitted,".into()), Line { before: Some(6.0), ..sig(firm("firmName")) }];
        lines.push(sig(firm("address1")));
        if !f.firm("address2").is_empty() {
            lines.push(sig(firm("address2")));
        }
        lines.push(sig(format!("{}, {} {}", f.firm("city"), f.firm("state"), f.firm("zip")).into()));
        lines.push(sig(format!("{} - ph", f.firm("phone")).into()));
        if !f.firm("fax").is_empty() {
            lines.push(sig(format!("{} - fax", f.firm("fax")).into()));
        }
        lines.push(sig(firm("email")));
        // "Attorney for Petitioner", or with the clients' names when the firm represents only
        // some of its side.
        let mut whom = Txt::from("Attorney for ").field(f, client_role_key(f));
        let clients = client_keys(f);
        if clients.len() != f.client().parties.len() {
            whom = whom.lit(" ").names(f, &clients);
        }
        lines.push(Line { before: Some(12.0), after: Some(12.0), ..sig(whom) });
        lines.push(Line { before: Some(18.0), ..sig(format!("By: /s/ {}", f.firm("attorneyName")).into()) });
        lines.push(sig(format!("{}, MO Bar No. {}", f.firm("attorneyName"), f.firm("barNumber")).into()));
        self.block(lines)
    }

    /// The certificate of service: e-filing to counsel of record, and USPS to each pro se party.
    fn certificate(&mut self, f: &Form) -> Result<(), CmdError> {
        let mut said = Txt::default();
        let on_date = || Txt::from("I hereby certify that on this ").field(f, "date:long");
        if f.counsel || f.mail.is_empty() {
            said = on_date().lit(", an electronic copy of the foregoing was sent through the Missouri e-Filing system to counsel of record.");
        }
        for m in &f.mail {
            let first = said.text.is_empty();
            said = if first { on_date().lit(", a copy of this ") } else { said.lit(" A copy of this ") };
            said = said
                .field(f, "title")
                .lit(" was sent to ")
                .field(f, &m.name_key)
                .lit(" postage pre-paid via USPS to last known address of ")
                .field(f, &m.addr_key)
                .lit(if first { "." } else { ", on today's date." });
        }
        let attorney = f.firm("attorneyName");
        self.block(vec![
            line("Certificate Heading", "Certificate of Service"),
            line("Certificate", said),
            Line { before: Some(18.0), ..line("Certificate", format!("/s/ {attorney}")) },
            line("Certificate", attorney),
        ])
    }

    /// The client's verification of a petition, with its notary block.
    fn verification(&mut self, f: &Form) -> Result<(), CmdError> {
        let clients = client_keys(f);
        let rule = "___________________________";
        self.block(vec![
            Line { bold: true, ..line("Certificate", "VERIFICATION") },
            line(
                "Certificate",
                Txt::from("The undersigned, ")
                    .names(f, &clients)
                    .lit(", states that the facts set forth in the foregoing Petition are true and correct to the best of my knowledge and belief."),
            ),
            Line { before: Some(18.0), ..line("Certificate", rule) },
            line("Certificate", Txt::default().names(f, &clients).lit(", ").field(f, client_role_key(f))),
            line("Certificate", "State of Missouri"),
            line("Certificate", Txt::from("County of ").field(f, "county:tc")),
            line("Certificate", "Subscribed and sworn to before me this _______ day of _____________, 20_____."),
            Line { before: Some(18.0), ..line("Certificate", rule) },
            line("Certificate", "Notary Public"),
            line("Certificate", "My Commission Expires: ___________"),
        ])
    }
}
