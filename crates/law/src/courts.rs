//! Saved courts: a court an attorney files in often, entered once and picked from a list in New
//! Pleading. `law.courts.list`, `law.courts.save` and `law.courts.delete` keep them in
//! `courts.json` in the court profiles folder ([`crate::store`]), like the firm details.
//!
//! A saved court is a few words for the caption, not a contact record: LawCraft doesn't keep
//! judges, clerks or opposing counsel (contacts come from other programs, [`crate::contacts`]).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wordcraft_engine::{CmdError, CmdResult, Session, p};

use crate::cmd::store;

/// Most courts in the list.
pub const MAX_COURTS: usize = 500;
/// Longest value, in characters.
const MAX_VALUE: usize = 200;

/// One saved court.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Court {
    /// Made from the name when the court is first saved.
    pub id: String,
    /// What the list shows ("Adair County Circuit").
    pub name: String,
    /// CIRCUIT, ASSOCIATE CIRCUIT…
    pub court_type: String,
    pub county: String,
    /// The state the court sits in; New Pleading writes Missouri pleadings today.
    #[serde(default = "missouri")]
    pub state: String,
    /// The caption's division line, when the court has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub division: String,
    /// The court profile pleadings in this court are formatted with.
    #[serde(default = "default_profile")]
    pub profile: String,
}

fn missouri() -> String {
    "MISSOURI".into()
}

fn default_profile() -> String {
    crate::pleadings::PROFILE.into()
}

/// What `law.courts.save` takes: a court, its id optional.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    #[serde(default)]
    id: Option<String>,
    name: String,
    court_type: String,
    county: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    division: Option<String>,
    #[serde(default)]
    profile: Option<String>,
}

impl Court {
    /// The problem with a court's values, if any: required ones empty, or too long.
    pub fn check(&self) -> Result<(), String> {
        for (key, value, required) in [
            ("id", &self.id, true),
            ("name", &self.name, true),
            ("courtType", &self.court_type, true),
            ("county", &self.county, true),
            ("state", &self.state, true),
            ("division", &self.division, false),
            ("profile", &self.profile, true),
        ] {
            if required && value.trim().is_empty() {
                return Err(format!("`{key}` is required"));
            }
            if value.chars().count() > MAX_VALUE {
                return Err(format!("`{key}` is longer than {MAX_VALUE} characters"));
            }
        }
        Ok(())
    }
}

/// An id from a name: lower-case letters and digits, joined by dashes ("adair-county-circuit").
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(60).collect::<String>();
    if out.is_empty() { "court".into() } else { out }
}

/// Save a court (`law.courts.save`'s params): a court with the same `id`, or with the same name
/// when no id is given, is replaced; otherwise it's added. Returns the court as saved.
pub fn save(v: &Value) -> Result<Value, String> {
    let input: Input = serde_json::from_value(v.clone()).map_err(|e| format!("not a court: {e}"))?;
    let trim = |t: Option<String>| t.map(|t| t.trim().to_string());
    let mut store = store();
    let mut courts = store.courts().to_vec();
    let name = input.name.trim().to_string();
    let at = match trim(input.id).filter(|t| !t.is_empty()) {
        Some(id) => Some(courts.iter().position(|c| c.id == id).ok_or_else(|| format!("no saved court `{id}`"))?),
        None => courts.iter().position(|c| c.name.eq_ignore_ascii_case(&name)),
    };
    let id = match at {
        Some(i) => courts[i].id.clone(),
        None => {
            let base = slug(&name);
            let mut id = base.clone();
            let mut n = 2;
            while courts.iter().any(|c| c.id == id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        }
    };
    let court = Court {
        id,
        name,
        court_type: input.court_type.trim().to_string(),
        county: input.county.trim().to_string(),
        state: trim(input.state).filter(|t| !t.is_empty()).unwrap_or_else(missouri),
        division: trim(input.division).unwrap_or_default(),
        profile: trim(input.profile).filter(|t| !t.is_empty()).unwrap_or_else(default_profile),
    };
    court.check()?;
    if store.get(&court.profile).is_none() {
        return Err(format!("no court profile `{}`", court.profile));
    }
    match at {
        Some(i) => courts[i] = court.clone(),
        None if courts.len() >= MAX_COURTS => return Err(format!("the list holds at most {MAX_COURTS} courts")),
        None => courts.push(court.clone()),
    }
    store.set_courts(courts).map_err(|e| e.to_string())?;
    Ok(json!(court))
}

/// The saved court of this id, as JSON.
pub fn get(id: &str) -> Option<Value> {
    store().courts().iter().find(|c| c.id == id).map(|c| json!(c))
}

/// `law.courts.list`: the saved courts, by name.
pub fn list_cmd(_: &mut Session, _: &Value) -> CmdResult {
    let mut courts = store().courts().to_vec();
    courts.sort_by_key(|c| c.name.to_lowercase());
    Ok(json!(courts))
}

/// `law.courts.save`.
pub fn save_cmd(_: &mut Session, v: &Value) -> CmdResult {
    save(v).map_err(CmdError::Params)
}

/// `law.courts.delete` `{id}` → `{"deleted": id}`.
pub fn delete_cmd(_: &mut Session, v: &Value) -> CmdResult {
    let id = p::req_str(v, "id")?;
    let mut store = store();
    let mut courts = store.courts().to_vec();
    let Some(i) = courts.iter().position(|c| c.id == id) else { return Err(CmdError::Params(format!("no saved court `{id}`"))) };
    courts.remove(i);
    store.set_courts(courts).map_err(|e| CmdError::Failed(e.to_string()))?;
    Ok(json!({"deleted": id}))
}
