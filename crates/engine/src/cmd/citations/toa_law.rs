//! LawCraft table-of-authorities helpers: TA marks, category filter, and `\p` passim pages.
//!
//! Kept out of WordCraft's [`super::generated`] so TOA category/passim stay in one place.

use wordcraft_doc::para::InlineObject;

/// A citation mark (Word's `TA` field) for `entry`, in `category` (TA `\c`) when given. The
/// field is hidden text, as References › Mark Citation inserts it.
pub fn ta_field(entry: &str, category: Option<u8>) -> InlineObject {
    let entry = entry.replace('"', "");
    let instr = match category {
        Some(c) => format!("TA \\l \"{entry}\" \\c {c}"),
        None => format!("TA \\l \"{entry}\""),
    };
    InlineObject::Field { instr, result: String::new(), locked: false, dirty: false, code: false, form: String::new() }
}

/// The value of a field switch (`\c "4"` or `\c 4` → `4`), if the instruction has it.
pub fn switch_value(instr: &str, switch: char) -> Option<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in instr.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                if !quoted {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let want = format!("\\{switch}");
    let i = tokens.iter().position(|t| t.eq_ignore_ascii_case(&want))?;
    tokens.get(i + 1).cloned()
}

/// Whether the field instruction has a switch (`\p`).
pub fn has_switch(instr: &str, switch: char) -> bool {
    let want = format!("\\{switch}");
    instr.split_whitespace().any(|t| t.eq_ignore_ascii_case(&want))
}

/// Pages at which a table of authorities with `\p` prints "passim" instead (Word: five).
const PASSIM_PAGES: usize = 5;

/// Category filter and passim formatting for one generated-list field instruction.
pub(super) struct Context {
    category: Option<String>,
    passim: bool,
}

impl Context {
    /// Options implied by a TOA (or other) field instruction; non-TOA kinds accept every mark.
    pub(super) fn for_kind(kind: &str) -> Self {
        Self {
            category: kind.starts_with("TOA").then(|| switch_value(kind, 'c')).flatten(),
            passim: kind.starts_with("TOA") && has_switch(kind, 'p'),
        }
    }

    /// Whether a TA/XE mark instruction belongs in this table (TOA `\c` category).
    pub(super) fn include_mark(&self, ta: &str) -> bool {
        self.category.as_deref().is_none_or(|c| switch_value(ta, 'c').as_deref().unwrap_or("1") == c)
    }

    /// Page list for an authority, or `"passim"` when `\p` and enough pages.
    pub(super) fn format_pages(&self, pages: &[u32]) -> String {
        if self.passim && pages.len() >= PASSIM_PAGES {
            "passim".to_string()
        } else {
            pages.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_switch_values() {
        assert_eq!(switch_value(r#"TOA \h \c "4" \p"#, 'c').as_deref(), Some("4"));
        assert_eq!(switch_value(r#"TA \l "Doe, 1 U.S. 1 \c" \c 2"#, 'c').as_deref(), Some("2"));
        assert_eq!(switch_value("TOA", 'c'), None);
        assert!(has_switch(r#"TOA \c "1" \p"#, 'p') && !has_switch(r#"TOA \c "1""#, 'p'));
    }

    #[test]
    fn category_and_passim() {
        let toa = Context::for_kind(r#"TOA \c "2" \p"#);
        assert!(toa.include_mark(r#"TA \l "A" \c 2"#));
        assert!(!toa.include_mark(r#"TA \l "A" \c 1"#));
        assert!(!toa.include_mark(r#"TA \l "A""#)); // bare TA is category 1
        assert_eq!(toa.format_pages(&[1, 2, 3, 4, 5]), "passim");
        assert_eq!(toa.format_pages(&[1, 2]), "1, 2");
        let plain = Context::for_kind("INDEX");
        assert!(plain.include_mark("XE \"x\""));
        assert_eq!(plain.format_pages(&[1, 2, 3, 4, 5]), "1, 2, 3, 4, 5");
    }
}
