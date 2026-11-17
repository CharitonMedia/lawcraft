//! Appendix page-number prefix helpers for [`super::page_number`].

use super::{Align, CharProps, CmdError, InlineObject, Paragraph, Session, TabAlign, TabStop, Value, p};

/// Build the gallery page-number paragraph, optionally with a text `prefix` before PAGE.
pub(super) fn page_number_paragraph(style: &str, align: Align, x_of_y: bool, prefix: &str) -> Result<Paragraph, CmdError> {
    let field = InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() };
    let mut para = Paragraph::new().styled(style);
    para.props.align = Some(align);
    para.props.tabs = Some(vec![TabStop { pos: 0.0, align: TabAlign::Clear, leader: Default::default() }]);
    if x_of_y {
        let page_pos = if prefix.is_empty() {
            para.insert_text(0, "Page  of ", &CharProps::default())?;
            5
        } else {
            para.insert_text(0, prefix, &CharProps::default())?;
            prefix.len()
        };
        para.insert_object(page_pos, field, &CharProps { bold: Some(true), ..Default::default() })?;
        if !prefix.is_empty() {
            para.insert_text(para.len(), " of ", &CharProps::default())?;
        }
        let end = para.len();
        para.insert_object(
            end,
            InlineObject::Field { instr: "NUMPAGES".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() },
            &CharProps { bold: Some(true), ..Default::default() },
        )?;
    } else if prefix.is_empty() {
        para.insert_object(0, field, &CharProps::default())?;
    } else {
        para.insert_text(0, prefix, &CharProps::default())?;
        para.insert_object(prefix.len(), field, &CharProps::default())?;
    }
    Ok(para)
}

/// Validate and return the optional `prefix` param (at most 16 characters).
pub(super) fn read_prefix<'a>(v: &'a Value) -> Result<Option<&'a str>, CmdError> {
    let Some(prefix) = p::str(v, "prefix") else {
        return Ok(None);
    };
    if prefix.chars().count() > 16 {
        return Err(CmdError::Params("`prefix` is at most 16 characters".into()));
    }
    Ok(Some(prefix))
}

/// Store the section's page-number prefix when the command gave one.
pub(super) fn apply_section_prefix(s: &mut Session, prefix_param: Option<&str>) -> Result<(), CmdError> {
    let Some(pfx) = prefix_param else {
        return Ok(());
    };
    crate::cmd::page::with_sect(s, |sect| {
        sect.page_num_prefix = if pfx.is_empty() { None } else { Some(pfx.to_string()) };
    })?;
    Ok(())
}
