//! Contacts from other programs: `law.contacts.read` reads a vCard file (.vcf, as Outlook,
//! Google Contacts, Apple Contacts, Clio and MyCase export them) so New Pleading can fill a
//! party's name and mailing address from it. Nothing is kept: the file is read once and only
//! the fields filled in stay, in the form.
//!
//! vCard 3.0 and 4.0 (RFC 2426, RFC 6350): `FN` (or `N`), `ORG`, `ADR`, `EMAIL` and `TEL`, with
//! folded lines, groups (`item1.ADR`), escapes and several cards in one file.

use serde_json::{Value, json};
use wordcraft_engine::{CmdError, CmdResult, Session, p};

/// Largest file read, in bytes.
pub const MAX_BYTES: u64 = 1 << 20;
/// Most cards in one file.
pub const MAX_CARDS: usize = 1000;

/// One contact: what New Pleading can fill from it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contact {
    pub name: String,
    pub organization: String,
    /// The mailing address, one line per address line.
    pub address: String,
    pub email: String,
    pub phone: String,
}

impl Contact {
    fn json(&self) -> Value {
        json!({"name": self.name, "organization": self.organization, "address": self.address, "email": self.email, "phone": self.phone})
    }
}

/// Undo vCard text escapes: `\n` a new line, `\,` `\;` `\\` the character itself.
fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// Split a structured value on unescaped `;`, each part unescaped.
fn components(v: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for c in v.chars() {
        if esc {
            cur.push('\\');
            cur.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == ';' {
            parts.push(unescape(&cur));
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    parts.push(unescape(&cur));
    parts
}

/// One content line: its name (upper case, group dropped), parameters (upper case) and value.
fn content_line(line: &str) -> Option<(String, String, &str)> {
    // The value starts at the first colon outside a quoted parameter value.
    let mut quoted = false;
    let colon = line.char_indices().find(|(_, c)| {
        if *c == '"' {
            quoted = !quoted;
        }
        *c == ':' && !quoted
    })?;
    let (head, value) = (&line[..colon.0], &line[colon.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?;
    let name = name.rsplit('.').next().unwrap_or(name).trim().to_ascii_uppercase();
    let params = parts.collect::<Vec<_>>().join(";").to_ascii_uppercase();
    Some((name, params, value))
}

/// Whether a property's parameters mark it preferred or for work.
fn preferred(params: &str) -> bool {
    params.contains("PREF") || params.contains("WORK")
}

/// Parse the cards in a vCard file's text. Cards with neither a name nor an organization are
/// left out.
pub fn parse(text: &str) -> Result<Vec<Contact>, String> {
    // Unfold: a line starting with a space or tab continues the one before.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match raw.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => {
                if let Some(last) = lines.last_mut() {
                    last.push_str(rest);
                }
            }
            _ => lines.push(raw.to_string()),
        }
    }
    let mut out = Vec::new();
    let mut card: Option<(Contact, String, bool, bool)> = None; // contact, N name, adr preferred, has adr
    let mut cards = 0;
    for l in &lines {
        let Some((name, params, value)) = content_line(l) else { continue };
        match name.as_str() {
            "BEGIN" if value.trim().eq_ignore_ascii_case("VCARD") => {
                cards += 1;
                if cards > MAX_CARDS {
                    return Err(format!("the file has more than {MAX_CARDS} contacts"));
                }
                card = Some((Contact::default(), String::new(), false, false));
            }
            "END" if value.trim().eq_ignore_ascii_case("VCARD") => {
                if let Some((mut c, n, _, _)) = card.take() {
                    if c.name.is_empty() {
                        c.name = n;
                    }
                    if c.name.is_empty() {
                        c.name = c.organization.clone();
                    }
                    if !c.name.is_empty() {
                        out.push(c);
                    }
                }
            }
            _ => {
                let Some((c, n, adr_pref, has_adr)) = card.as_mut() else { continue };
                match name.as_str() {
                    "FN" if c.name.is_empty() => c.name = unescape(value).trim().to_string(),
                    "N" if n.is_empty() => {
                        // Family; Given; Additional; Prefixes; Suffixes.
                        let p = components(value);
                        let get = |i: usize| p.get(i).map(|s| s.replace(',', " ").trim().to_string()).unwrap_or_default();
                        let parts = [get(3), get(1), get(2), get(0), get(4)];
                        *n = parts.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
                    }
                    "ORG" if c.organization.is_empty() => {
                        c.organization =
                            components(value).into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
                    }
                    "ADR" if !*has_adr || (preferred(&params) && !*adr_pref) => {
                        // PO box; extended; street; locality; region; postal code; country.
                        let p = components(value);
                        let get = |i: usize| p.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
                        let mut lines: Vec<String> = Vec::new();
                        for part in [get(0), get(1), get(2)] {
                            lines.extend(part.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from));
                        }
                        let city = [get(3), get(4)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
                        let city_line = [city, get(5)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
                        if !city_line.is_empty() {
                            lines.push(city_line);
                        }
                        if !get(6).is_empty() {
                            lines.push(get(6));
                        }
                        if !lines.is_empty() {
                            c.address = lines.join("\n");
                            *has_adr = true;
                            *adr_pref = preferred(&params);
                        }
                    }
                    "EMAIL" if c.email.is_empty() => c.email = unescape(value).trim().to_string(),
                    "TEL" if c.phone.is_empty() => {
                        let t = unescape(value);
                        c.phone = t.trim().strip_prefix("tel:").unwrap_or(t.trim()).to_string();
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(out)
}

/// `law.contacts.read` `{"path"}` → the contacts in a vCard file.
pub fn read_cmd(_: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    let contacts = read(path).map_err(CmdError::Failed)?;
    Ok(Value::Array(contacts.iter().map(Contact::json).collect()))
}

/// Read and parse a vCard file (at most 1 MB; inside the automation read roots when confined).
pub fn read(path: &str) -> Result<Vec<Contact>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let bytes = wordcraft_engine::io::read_file_capped(path, MAX_BYTES, "contact file")?;
        parse(&String::from_utf8_lossy(&bytes))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Err("reading files isn't available in this build".into())
    }
}
