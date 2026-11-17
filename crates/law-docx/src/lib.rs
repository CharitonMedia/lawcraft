//! LawCraft helpers for Word packages: appendix page-number prefixes stored in `w:docVars`.
//!
//! `w:sectPr` has no extension slot for a page-number prefix, so LawCraft keeps each section's
//! [`wordcraft_doc::SectionProps::page_num_prefix`] in Word's document variables under names
//! starting with [`wordcraft_doc::section::PAGE_NUM_PREFIX_DOCVAR`]. WordCraft's `wordcraft-docx`
//! crate does not know about that convention; these functions rewrite or read `word/settings.xml`
//! in the OOXML package bytes around a normal docx read/write.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{Cursor, Read, Write};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use wordcraft_doc::Document;
use wordcraft_doc::section::PAGE_NUM_PREFIX_DOCVAR;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Most `w:docVars` document variables read from or written to a file (`w:settings/w:docVars`).
pub const MAX_DOC_VARS: usize = 4096;
/// Longest `w:docVar` name kept (ECMA-376 §17.15.1.20: at most 255 characters).
pub const MAX_DOC_VAR_NAME: usize = 255;
/// Longest `w:docVar` value kept.
pub const MAX_DOC_VAR_VALUE: usize = 4096;
/// Longest page-number prefix kept on a section.
const MAX_PAGE_NUM_PREFIX: usize = 16;

const SETTINGS_PART: &str = "word/settings.xml";

/// Errors from reading or rewriting page-number prefix document variables in a package.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum LawDocxError {
    /// The bytes are not a readable zip archive.
    #[error("not a valid zip package: {0}")]
    Zip(String),
    /// A part is not well-formed XML, or rewriting it failed.
    #[error("malformed XML: {0}")]
    Xml(String),
    /// The input exceeds a safety limit.
    #[error("limit exceeded: {0}")]
    Limit(String),
}

/// Apply per-section page-number prefixes from `w:docVars` in the package's `word/settings.xml`
/// onto `doc`. Missing settings is a no-op. Reserved names are consumed into
/// [`wordcraft_doc::SectionProps::page_num_prefix`]; every other variable is synced into
/// `doc.settings.doc_vars` (replacing that list).
pub fn apply_page_num_prefixes(doc: &mut Document, package: &[u8]) -> Result<(), LawDocxError> {
    let Some(settings) = read_settings_xml(package)? else {
        return Ok(());
    };
    let vars = parse_doc_vars(&settings)?;
    doc.settings.doc_vars.clear();
    let mut prefixes: Vec<(usize, String)> = Vec::new();
    for (name, val) in vars {
        match reserved_section_index(&name) {
            Some(idx) => {
                if let Some(prefix) = page_num_prefix_val(Some(&val)) {
                    prefixes.push((idx, prefix));
                }
            }
            None => {
                if doc.settings.doc_vars.len() < MAX_DOC_VARS {
                    doc.settings.doc_vars.push((name, val));
                }
            }
        }
    }
    if prefixes.is_empty() {
        return Ok(());
    }
    let ends: Vec<usize> = doc.sections().iter().map(|(e, _)| *e).collect();
    for (idx, prefix) in prefixes {
        if let Some(end) = ends.get(idx).copied() {
            doc.section_mut(end).page_num_prefix = Some(prefix);
        }
    }
    Ok(())
}

/// Rewrite `word/settings.xml` in `package` so reserved page-number-prefix `w:docVar` entries
/// match `doc`'s sections exactly. Non-reserved variables come from `doc.settings.doc_vars`
/// (stale reserved names there are ignored). Other settings children are left untouched.
pub fn write_page_num_prefixes(package: &mut Vec<u8>, doc: &Document) -> Result<(), LawDocxError> {
    let section_vars = section_prefix_vars(doc);
    let reserved = PAGE_NUM_PREFIX_DOCVAR;
    let saved: Vec<(String, String)> = doc
        .settings
        .doc_vars
        .iter()
        .filter(|(n, _)| !n.starts_with(reserved))
        .take(MAX_DOC_VARS.saturating_sub(section_vars.len()))
        .map(|(n, v)| (n.clone(), v.clone()))
        .collect();
    let settings = match read_settings_xml(package)? {
        Some(s) => s,
        None if saved.is_empty() && section_vars.is_empty() => return Ok(()),
        None => {
            let xml = minimal_settings_with_doc_vars(&saved, &section_vars);
            return replace_settings_xml(package, xml.into_bytes());
        }
    };
    let next = rewrite_settings_doc_vars(&settings, &saved, &section_vars)?;
    replace_settings_xml(package, next.into_bytes())
}

fn page_num_prefix_val(raw: Option<&str>) -> Option<String> {
    let p = raw?.chars().take(MAX_PAGE_NUM_PREFIX).collect::<String>();
    if p.is_empty() { None } else { Some(p) }
}

fn reserved_section_index(name: &str) -> Option<usize> {
    name.strip_prefix(PAGE_NUM_PREFIX_DOCVAR).filter(|n| !n.is_empty()).and_then(|n| n.parse::<usize>().ok())
}

fn section_prefix_vars(doc: &Document) -> Vec<(String, String)> {
    doc.sections()
        .iter()
        .enumerate()
        .filter_map(|(i, (_, sp))| {
            let prefix = sp.page_num_prefix.as_deref().filter(|p| !p.is_empty())?;
            if prefix.chars().count() > MAX_DOC_VAR_VALUE {
                return None;
            }
            Some((format!("{PAGE_NUM_PREFIX_DOCVAR}{i}"), prefix.to_string()))
        })
        .take(MAX_DOC_VARS)
        .collect()
}

fn read_settings_xml(package: &[u8]) -> Result<Option<String>, LawDocxError> {
    let mut zip = ZipArchive::new(Cursor::new(package)).map_err(|e| LawDocxError::Zip(e.to_string()))?;
    let idx = match find_settings_index(&mut zip) {
        Some(i) => i,
        None => return Ok(None),
    };
    let mut file = zip.by_index(idx).map_err(|e| LawDocxError::Zip(e.to_string()))?;
    let mut xml = String::new();
    file.read_to_string(&mut xml).map_err(|e| LawDocxError::Zip(e.to_string()))?;
    Ok(Some(xml))
}

fn find_settings_index<R: Read + std::io::Seek>(zip: &mut ZipArchive<R>) -> Option<usize> {
    for i in 0..zip.len() {
        let Ok(f) = zip.by_index(i) else { continue };
        if f.name().eq_ignore_ascii_case(SETTINGS_PART) {
            return Some(i);
        }
    }
    None
}

fn parse_doc_vars(settings_xml: &str) -> Result<Vec<(String, String)>, LawDocxError> {
    let mut reader = Reader::from_str(settings_xml);
    reader.config_mut().trim_text(true);
    let mut out: Vec<(String, String)> = Vec::new();
    let mut buf = Vec::new();
    let mut in_doc_vars = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let qname = e.name().as_ref().to_vec();
                let name = local_name(&qname);
                if name == "docVars" {
                    in_doc_vars = true;
                } else if in_doc_vars && name == "docVar" {
                    if out.len() >= MAX_DOC_VARS {
                        break;
                    }
                    if let Some(pair) = doc_var_attrs(&e) {
                        out.push(pair);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let qname = e.name().as_ref().to_vec();
                if local_name(&qname) == "docVars" {
                    in_doc_vars = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(LawDocxError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

fn doc_var_attrs(e: &BytesStart<'_>) -> Option<(String, String)> {
    let mut name = None;
    let mut val = None;
    for a in e.attributes().flatten() {
        let key_bytes = a.key.as_ref().to_vec();
        let key = local_name(&key_bytes);
        let value = String::from_utf8_lossy(&a.value);
        if key == "name" {
            name = Some(value.chars().take(MAX_DOC_VAR_NAME).collect::<String>());
        } else if key == "val" {
            val = Some(value.chars().take(MAX_DOC_VAR_VALUE).collect::<String>());
        }
    }
    let name = name.filter(|n| !n.is_empty())?;
    let val = val.unwrap_or_default();
    Some((name, val))
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    s.rsplit_once(':').map(|(_, l)| l).unwrap_or(s)
}

fn rewrite_settings_doc_vars(settings: &str, saved: &[(String, String)], section_vars: &[(String, String)]) -> Result<String, LawDocxError> {
    let block = doc_vars_xml(saved, section_vars);
    if let Some((start, end)) = find_element_span(settings, "docVars") {
        let mut out = String::with_capacity(settings.len().saturating_add(block.len()));
        out.push_str(settings.get(..start).unwrap_or(""));
        out.push_str(&block);
        out.push_str(settings.get(end..).unwrap_or(""));
        return Ok(out);
    }
    if block.is_empty() {
        return Ok(settings.to_string());
    }
    // Schema order: after `w:compat`, before `w:rsids` (ECMA-376 CT_Settings).
    if let Some((_, end)) = find_element_span(settings, "compat") {
        let mut out = String::with_capacity(settings.len().saturating_add(block.len()));
        out.push_str(settings.get(..end).unwrap_or(""));
        out.push_str(&block);
        out.push_str(settings.get(end..).unwrap_or(""));
        return Ok(out);
    }
    if let Some((start, _)) = find_element_span(settings, "rsids") {
        let mut out = String::with_capacity(settings.len().saturating_add(block.len()));
        out.push_str(settings.get(..start).unwrap_or(""));
        out.push_str(&block);
        out.push_str(settings.get(start..).unwrap_or(""));
        return Ok(out);
    }
    if let Some(close) = settings.rfind("</w:settings>") {
        let mut out = String::with_capacity(settings.len().saturating_add(block.len()));
        out.push_str(settings.get(..close).unwrap_or(""));
        out.push_str(&block);
        out.push_str(settings.get(close..).unwrap_or(""));
        return Ok(out);
    }
    if let Some(close) = settings.rfind("</settings>") {
        let mut out = String::with_capacity(settings.len().saturating_add(block.len()));
        out.push_str(settings.get(..close).unwrap_or(""));
        out.push_str(&block);
        out.push_str(settings.get(close..).unwrap_or(""));
        return Ok(out);
    }
    Err(LawDocxError::Xml("settings root close tag not found".into()))
}

fn doc_vars_xml(saved: &[(String, String)], section_vars: &[(String, String)]) -> String {
    if saved.is_empty() && section_vars.is_empty() {
        return String::new();
    }
    let mut s = String::from("<w:docVars>");
    for (name, val) in saved.iter().chain(section_vars.iter()) {
        s.push_str("<w:docVar w:name=\"");
        push_xml_attr(&mut s, name);
        s.push_str("\" w:val=\"");
        push_xml_attr(&mut s, val);
        s.push_str("\"/>");
    }
    s.push_str("</w:docVars>");
    s
}

fn minimal_settings_with_doc_vars(saved: &[(String, String)], section_vars: &[(String, String)]) -> String {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    );
    s.push_str(&doc_vars_xml(saved, section_vars));
    s.push_str("</w:settings>");
    s
}

fn push_xml_attr(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

/// Byte span `[start, end)` of the first element whose local name is `local` (start or empty tag
/// through its matching end, or the empty tag alone).
fn find_element_span(xml: &str, local: &str) -> Option<(usize, usize)> {
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(rel) = find_byte(bytes, i, b'<') else { break };
        i = rel;
        let after = i.saturating_add(1);
        if bytes.get(after) == Some(&b'/') || bytes.get(after) == Some(&b'!') || bytes.get(after) == Some(&b'?') {
            i = after;
            continue;
        }
        let name_start = after;
        let name_end = scan_name_end(bytes, name_start)?;
        let qname = std::str::from_utf8(bytes.get(name_start..name_end)?).ok()?;
        let this_local = qname.rsplit_once(':').map(|(_, l)| l).unwrap_or(qname);
        if this_local != local {
            i = name_end;
            continue;
        }
        let tag_close = scan_tag_close(bytes, name_end)?;
        if bytes.get(tag_close.saturating_sub(1)) == Some(&b'/') {
            return Some((i, tag_close.saturating_add(1)));
        }
        let end = find_matching_end(bytes, tag_close.saturating_add(1), this_local)?;
        return Some((i, end));
    }
    None
}

fn find_byte(bytes: &[u8], from: usize, needle: u8) -> Option<usize> {
    bytes.get(from..)?.iter().position(|b| *b == needle).map(|p| from.saturating_add(p))
}

fn scan_name_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(&b) = bytes.get(i) {
        if b.is_ascii_whitespace() || b == b'>' || b == b'/' {
            return Some(i);
        }
        i = i.saturating_add(1);
    }
    None
}

fn scan_tag_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut i = from;
    while let Some(&b) = bytes.get(i) {
        match (quote, b) {
            (Some(q), b) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'>') => return Some(i),
            _ => {}
        }
        i = i.saturating_add(1);
    }
    None
}

fn find_matching_end(bytes: &[u8], mut i: usize, local: &str) -> Option<usize> {
    let mut depth = 1usize;
    while i < bytes.len() {
        let Some(rel) = find_byte(bytes, i, b'<') else { return None };
        i = rel;
        let after = i.saturating_add(1);
        if bytes.get(after) == Some(&b'!') || bytes.get(after) == Some(&b'?') {
            i = after;
            continue;
        }
        let closing = bytes.get(after) == Some(&b'/');
        let name_start = if closing { after.saturating_add(1) } else { after };
        let name_end = scan_name_end(bytes, name_start)?;
        let qname = std::str::from_utf8(bytes.get(name_start..name_end)?).ok()?;
        let this_local = qname.rsplit_once(':').map(|(_, l)| l).unwrap_or(qname);
        let tag_close = scan_tag_close(bytes, name_end)?;
        let empty = !closing && bytes.get(tag_close.saturating_sub(1)) == Some(&b'/');
        if this_local == local {
            if closing {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(tag_close.saturating_add(1));
                }
            } else if !empty {
                depth = depth.saturating_add(1);
            }
        }
        i = tag_close.saturating_add(1);
    }
    None
}

fn replace_settings_xml(package: &mut Vec<u8>, settings: Vec<u8>) -> Result<(), LawDocxError> {
    let entries = {
        let mut src = ZipArchive::new(Cursor::new(package.as_slice())).map_err(|e| LawDocxError::Zip(e.to_string()))?;
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        for i in 0..src.len() {
            let mut file = src.by_index(i).map_err(|e| LawDocxError::Zip(e.to_string()))?;
            let name = file.name().to_string();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|e| LawDocxError::Zip(e.to_string()))?;
            entries.push((name, bytes));
        }
        entries
    };
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated).compression_level(Some(6));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut wrote_settings = false;
    for (name, bytes) in &entries {
        let is_settings = name.eq_ignore_ascii_case(SETTINGS_PART);
        let opts = if is_settings {
            wrote_settings = true;
            deflated
        } else {
            let lower = name.to_ascii_lowercase();
            if [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e)) { stored } else { deflated }
        };
        out.start_file(name.as_str(), opts).map_err(|e| LawDocxError::Zip(e.to_string()))?;
        if is_settings {
            out.write_all(&settings).map_err(|e| LawDocxError::Zip(e.to_string()))?;
        } else {
            out.write_all(bytes).map_err(|e| LawDocxError::Zip(e.to_string()))?;
        }
    }
    if !wrote_settings {
        out.start_file(SETTINGS_PART, deflated).map_err(|e| LawDocxError::Zip(e.to_string()))?;
        out.write_all(&settings).map_err(|e| LawDocxError::Zip(e.to_string()))?;
    }
    let finished = out.finish().map_err(|e| LawDocxError::Zip(e.to_string()))?;
    *package = finished.into_inner();
    Ok(())
}
