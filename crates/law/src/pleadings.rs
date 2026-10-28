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

use std::collections::BTreeMap;

use serde_json::{Value, json};
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
    let form = Form::read(v)?;
    // Before the profile goes on: its page-number footer isn't the person's text.
    if !is_blank(&s.doc) {
        return Err(CmdError::Params(NOT_BLANK.into()));
    }
    let Some(profile) = store().get(PROFILE).cloned() else { return Err(CmdError::Failed(format!("no court profile `{PROFILE}`"))) };
    crate::apply::apply(s, &profile, form.kind).map_err(|e| CmdError::Failed(e.to_string()))?;
    let sections = Writer { s, started: false, lists: Vec::new() }.write(&form)?;
    // File › Properties › Title: what a `TITLE` field in the footer shows (California's footer).
    s.join_next_undo();
    s.run("file.properties", &json!({"title": form.title}))?;
    Ok(json!({"profile": PROFILE, "type": form.kind, "sections": sections}))
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

/// The params, checked.
struct Form {
    kind: &'static str,
    title: String,
    court_type: String,
    county: String,
    case_number: String,
    first: Side,
    second: Side,
    /// The firm's clients are on the first side.
    client_first: bool,
    /// The parties served by USPS: name and mailing address.
    mail: Vec<(String, String)>,
    /// Some party the firm doesn't represent has counsel, served through e-filing.
    counsel: bool,
    date: (i64, u32, u32),
    verified: bool,
    firm: BTreeMap<String, String>,
}

impl Form {
    /// Read and check the command's params; the first problem is the error.
    fn read(v: &Value) -> Result<Form, CmdError> {
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
        if !petition {
            for p in first.parties.iter().chain(&second.parties).filter(|p| p.pro_se) {
                let Some(address) = &p.address else {
                    return Err(CmdError::Params(format!("`address` is required for pro se party {}", p.name)));
                };
                mail.push((p.name.clone(), address.clone()));
            }
            match text(v, "service")?.as_deref() {
                None | Some("efileCounsel") => {}
                Some("efileUsps") => mail.push((req(v, "servedParty")?, req(v, "servedAddress")?)),
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
        if firm.is_empty() {
            return Err(CmdError::Params("`firm` is required: no firm details are saved (law.firm.set)".into()));
        }
        if let Some((key, _)) = FIRM_FIELDS.iter().find(|(k, needed)| *needed && !firm.contains_key(*k)) {
            return Err(CmdError::Params(format!("`firm.{key}` is required")));
        }
        Ok(Form {
            kind,
            title: req(v, "title")?,
            court_type: text(v, "courtType")?.unwrap_or_else(|| "CIRCUIT".into()),
            county: req(v, "county")?,
            // A petition may be filed before a case number is assigned.
            case_number: if petition { text(v, "caseNumber")?.unwrap_or_default() } else { req(v, "caseNumber")? },
            first,
            second,
            client_first,
            mail,
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

    /// The clients' role, plural for more than one ("Respondents").
    fn client_role(&self) -> String {
        plural_role(&self.client().role, self.clients().len())
    }

    /// Who the signature block says the firm is attorney for: the role alone when the firm
    /// represents everyone on its side, else the role and the clients' names.
    fn for_whom(&self) -> String {
        let clients = self.clients();
        if clients.len() == self.client().parties.len() { self.client_role() } else { format!("{} {}", self.client_role(), join_names(&clients)) }
    }

    /// A firm detail by field name, empty when it isn't set.
    fn firm(&self, key: &str) -> &str {
        self.firm.get(key).map(String::as_str).unwrap_or("")
    }
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

/// One line of a block that stays on one page: its text and the space before and after it.
struct Line {
    style: &'static str,
    text: String,
    before: Option<f32>,
    after: Option<f32>,
    bold: bool,
}

/// A plain line of `text` in `style`, with no extra spacing and not bold.
fn line(style: &'static str, text: impl Into<String>) -> Line {
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

    /// A new paragraph in `style` holding `text`. A paragraph made by splitting the last one
    /// starts without its direct formatting.
    fn para(&mut self, style: &str, text: &str) -> Result<(), CmdError> {
        if self.started {
            self.run("text.insert", json!({"text": "\r", "raw": true}))?;
            self.run("para.reset", json!({}))?;
        }
        self.started = true;
        self.run("para.style", json!({"style": style}))?;
        if !text.is_empty() {
            self.run("text.insert", json!({"text": text, "raw": true}))?;
        }
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
    fn lead(&mut self, lead: &str, rest: &str) -> Result<(), CmdError> {
        self.para("Body", &format!("{lead}{rest}"))?;
        self.bold_lead(lead.len())
    }

    /// A paragraph numbered in a list (`numbered` 1., `lowerLetterDot` a.), numbered at the end.
    fn listed(&mut self, kind: &'static str, text: &str) -> Result<(), CmdError> {
        self.para("Numbered", text)?;
        self.lists.push((self.s.sel.focus.clone(), kind));
        Ok(())
    }

    /// Lines kept together on one page.
    fn block(&mut self, lines: &[Line]) -> Result<(), CmdError> {
        for (i, l) in lines.iter().enumerate() {
            self.para(l.style, &l.text)?;
            if l.bold {
                self.bold_lead(l.text.len())?;
            }
            if l.before.is_some() || l.after.is_some() {
                self.run("para.spacing", json!({"before": l.before, "after": l.after}))?;
            }
            self.run("para.keepLines", json!({"value": true}))?;
            if i + 1 < lines.len() {
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
        self.para("Title", &f.title)?;
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
        self.para("Court", &format!("IN THE {} COURT OF {} COUNTY", f.court_type, f.county))?;
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
        for (side, end) in [(&f.first, ","), (&f.second, ".")] {
            for p in &side.parties {
                self.para("Caption", &p.name.to_uppercase())?;
            }
            self.para("Caption", &format!("{}{end}", plural_role(&side.role, side.parties.len())))?;
            self.run("para.indents", json!({"left": 54}))?;
            if end == "," {
                self.para("Caption", "v.")?;
                self.run("para.indents", json!({"left": 18}))?;
                self.run("para.spacing", json!({"before": 8, "after": 8}))?;
            }
        }
        // Case number: 38%, 20 pt after the rule.
        self.cell(&cell(1), width * 0.38, [8.0, 20.0, 8.0, 0.0])?;
        let case = if f.case_number.is_empty() { "____________" } else { f.case_number.as_str() };
        self.para("Caption", &format!("Case No. {case}"))?;
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
        let clients = f.clients();
        let (names, role) = (join_names(&clients), f.client_role());
        let prays = if clients.len() > 1 { "pray" } else { "prays" };
        match f.kind {
            "notice" => self.lead(
                "PLEASE TAKE NOTICE",
                &format!(
                    " that [motion] will be called up for hearing on [date] at [time], or as soon thereafter as counsel may be heard, before the {} Court of {} County, Missouri.",
                    title_case(&f.court_type),
                    title_case(&f.county)
                ),
            ),
            "answer" => {
                self.lead("COMES NOW", &format!(" {names}, {role}, by and through counsel, and for Answer to the Petition states as follows:"))?;
                for n in 1..=3 {
                    self.listed("numbered", &format!("[Response to paragraph {n}.]"))?;
                }
                self.lead("WHEREFORE", &format!(", {role} {prays} this Court deny the relief requested in the Petition and grant such other and further relief as the Court deems just and proper."))
            }
            "petition" => {
                self.lead("COMES NOW", &format!(" {names}, {role}, by and through counsel, and for {} states as follows:", title_case(&f.title)))?;
                for _ in 0..3 {
                    self.listed("numbered", "[Allegation.]")?;
                }
                self.lead("WHEREFORE", &format!(", {role} {prays} that this Court:"))?;
                self.listed("lowerLetterDot", "[Relief requested.]")?;
                self.listed("lowerLetterDot", "Grant such other and further relief as the Court deems just and proper.")
            }
            _ => {
                self.lead("COMES NOW", &format!(" {names}, {role}, by and through counsel, and in support of this motion states as follows:"))?;
                for _ in 0..3 {
                    self.listed("numbered", "[Grounds.]")?;
                }
                self.lead(
                    "WHEREFORE",
                    &format!(", {role} {prays} this Court [state the relief requested] and for such other and further relief as the Court deems just and proper."),
                )
            }
        }
    }

    /// The signature block from the saved firm details.
    fn signature(&mut self, f: &Form) -> Result<(), CmdError> {
        let sig = |text: String| line("Signature", text);
        let mut lines = vec![sig("Respectfully Submitted,".into()), Line { before: Some(6.0), ..sig(f.firm("firmName").into()) }];
        lines.push(sig(f.firm("address1").into()));
        if !f.firm("address2").is_empty() {
            lines.push(sig(f.firm("address2").into()));
        }
        lines.push(sig(format!("{}, {} {}", f.firm("city"), f.firm("state"), f.firm("zip"))));
        lines.push(sig(format!("{} - ph", f.firm("phone"))));
        if !f.firm("fax").is_empty() {
            lines.push(sig(format!("{} - fax", f.firm("fax"))));
        }
        lines.push(sig(f.firm("email").into()));
        lines.push(Line { before: Some(12.0), after: Some(12.0), ..sig(format!("Attorney for {}", f.for_whom())) });
        lines.push(Line { before: Some(18.0), ..sig(format!("By: /s/ {}", f.firm("attorneyName"))) });
        lines.push(sig(format!("{}, MO Bar No. {}", f.firm("attorneyName"), f.firm("barNumber"))));
        self.block(&lines)
    }

    /// The certificate of service: e-filing to counsel of record, and USPS to each pro se party.
    fn certificate(&mut self, f: &Form) -> Result<(), CmdError> {
        let date = long_date(f.date);
        let mut said = String::new();
        if f.counsel || f.mail.is_empty() {
            said = format!(
                "I hereby certify that on this {date}, an electronic copy of the foregoing was sent through the Missouri e-Filing system to counsel of record."
            );
        }
        for (party, address) in &f.mail {
            if said.is_empty() {
                said = format!(
                    "I hereby certify that on this {date}, a copy of this {} was sent to {party} postage pre-paid via USPS to last known address of {address}.",
                    f.title
                );
            } else {
                said.push_str(&format!(
                    " A copy of this {} was sent to {party} postage pre-paid via USPS to last known address of {address}, on today's date.",
                    f.title
                ));
            }
        }
        let attorney = f.firm("attorneyName");
        self.block(&[
            line("Certificate Heading", "Certificate of Service"),
            line("Certificate", said),
            Line { before: Some(18.0), ..line("Certificate", format!("/s/ {attorney}")) },
            line("Certificate", attorney),
        ])
    }

    /// The client's verification of a petition, with its notary block.
    fn verification(&mut self, f: &Form) -> Result<(), CmdError> {
        let names = join_names(&f.clients());
        let rule = "___________________________";
        self.block(&[
            Line { bold: true, ..line("Certificate", "VERIFICATION") },
            line(
                "Certificate",
                format!("The undersigned, {names}, states that the facts set forth in the foregoing Petition are true and correct to the best of my knowledge and belief."),
            ),
            Line { before: Some(18.0), ..line("Certificate", rule) },
            line("Certificate", format!("{names}, {}", f.client_role())),
            line("Certificate", "State of Missouri"),
            line("Certificate", format!("County of {}", title_case(&f.county))),
            line("Certificate", "Subscribed and sworn to before me this _______ day of _____________, 20_____."),
            Line { before: Some(18.0), ..line("Certificate", rule) },
            line("Certificate", "Notary Public"),
            line("Certificate", "My Commission Expires: ___________"),
        ])
    }
}
