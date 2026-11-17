//! LawCraft's proofing facade: re-exports [`craft_proof`] and adds per-session [`ModuleEn`]
//! supplements (legal word list and grammar hooks) that craft-proof itself does not carry.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub use craft_proof::*;

use std::ops::Range;

/// English paragraph grammar check supplied by an optional module (LawCraft confusion pairs).
pub type EnParagraphGrammarCheck = fn(&str) -> Vec<Issue>;

/// Per-session English spelling and grammar supplements from an optional module.
///
/// Enabled and disabled sessions stay independent: the module is passed into
/// [`check_text_with_module`] rather than mutating global dictionaries.
#[derive(Clone, Copy, Default)]
pub struct ModuleEn {
    spelling: Option<&'static [&'static str]>,
    grammar: Option<EnParagraphGrammarCheck>,
}

impl std::fmt::Debug for ModuleEn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleEn")
            .field("spelling_words", &self.spelling.map(|w| w.len()))
            .field("grammar", &self.grammar.is_some())
            .finish()
    }
}

impl PartialEq for ModuleEn {
    fn eq(&self, other: &Self) -> bool {
        self.spelling == other.spelling && self.grammar.map(|f| f as usize) == other.grammar.map(|f| f as usize)
    }
}

impl Eq for ModuleEn {}

impl std::hash::Hash for ModuleEn {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.spelling.hash(state);
        self.grammar.map(|f| f as usize).hash(state);
    }
}

impl ModuleEn {
    /// Spelling supplement: lowercase base forms the module treats as correct.
    pub fn with_spelling(spelling: Option<&'static [&'static str]>) -> Self {
        ModuleEn { spelling, grammar: None }
    }

    /// Grammar hook run for English paragraphs after craft-proof's own checks.
    pub fn with_grammar(self, grammar: Option<EnParagraphGrammarCheck>) -> Self {
        ModuleEn { spelling: self.spelling, grammar }
    }
}

fn in_module_spelling(lower: &str, module: ModuleEn) -> bool {
    let Some(words) = module.spelling else {
        return false;
    };
    if words.binary_search(&lower).is_ok() {
        return true;
    }
    words.iter().any(|w| (*w).eq_ignore_ascii_case(lower))
}

fn is_english(lang: ProofLang) -> bool {
    matches!(lang, ProofLang::En | ProofLang::EnGb | ProofLang::EnAu | ProofLang::EnCa)
}

/// [`is_correct_with`] plus the module's spelling supplement.
pub fn is_correct_with_module(word: &str, opts: &Options, module: ModuleEn) -> bool {
    if is_correct_with(word, opts) {
        return true;
    }
    let lower = word.to_lowercase();
    in_module_spelling(&lower, module)
}

/// [`is_correct_in_with`] plus the module's spelling supplement (English only for the module list).
pub fn is_correct_in_with_module(word: &str, lang: ProofLang, opts: &Options, module: ModuleEn) -> bool {
    if is_correct_in_with(word, lang, opts) {
        return true;
    }
    if !is_english(lang) {
        return false;
    }
    let lower = word.to_lowercase();
    in_module_spelling(&lower, module)
}

/// [`check_spelling_with`], dropping issues for words the module lists as correct.
pub fn check_spelling_with_module(text: &str, opts: &Options, module: ModuleEn) -> Vec<Issue> {
    check_spelling_with(text, opts)
        .into_iter()
        .filter(|i| {
            if i.kind != IssueKind::Spelling {
                return true;
            }
            let Some(word) = text.get(i.start..i.end) else {
                return true;
            };
            !is_correct_with_module(word, opts, module)
        })
        .collect()
}

/// [`check_text_with`] plus module spelling suppressions and optional English grammar hook.
pub fn check_text_with_module(
    text: &str,
    spans: &[(Range<usize>, ProofLang)],
    opts: &Options,
    grammar: bool,
    module: ModuleEn,
) -> Vec<Issue> {
    check_paragraph(text, spans, opts, grammar, false, module)
}

/// Paragraph proofing with module supplements (layout's historical name).
pub fn check_paragraph(
    text: &str,
    spans: &[(Range<usize>, ProofLang)],
    opts: &Options,
    grammar: bool,
    list_item: bool,
    module: ModuleEn,
) -> Vec<Issue> {
    let mut issues = check_text_with(text, spans, opts, grammar, list_item);
    if module.spelling.is_some() {
        issues.retain(|i| {
            if i.kind != IssueKind::Spelling {
                return true;
            }
            let Some(word) = text.get(i.start..i.end) else {
                return true;
            };
            !is_correct_with_module(word, opts, module)
        });
    }
    if grammar {
        if let Some(hook) = module.grammar {
            // Run the module grammar over the whole paragraph text (LawCraft confusion pairs).
            for issue in hook(text) {
                issues.push(issue);
            }
            issues.sort_by_key(|i| (i.start, i.end));
        }
    }
    issues
}
