//! Wrap core WordCraft commands so LawCraft behaviour runs without patching those command files.

use std::sync::OnceLock;

use serde_json::Value;
use wordcraft_engine::{CmdError, CmdResult, Session};

/// Original `file.printPdf` run fn.
static PRINT_PDF: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
/// Original `review.issues` run fn.
static REVIEW_ISSUES: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();

fn wrap_print_pdf(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = PRINT_PDF.get() else {
        return Err(CmdError::Failed("file.printPdf wrap missing".into()));
    };
    let opts = wordcraft_engine::export_options(s, v);
    let findings = s.run_export_checks(&opts, "pdf");
    let mut out = orig(s, v)?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("findings".into(), Value::Array(findings));
    }
    Ok(out)
}

fn wrap_review_issues(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = REVIEW_ISSUES.get() else {
        return Err(CmdError::Failed("review.issues wrap missing".into()));
    };
    let core = orig(s, v)?;
    let mut out = s.lint_findings();
    if let Some(arr) = core.as_array() {
        out.extend(arr.iter().cloned());
    }
    if out.len() > 500 {
        out.truncate(500);
    }
    Ok(Value::Array(out))
}

fn capture_and_replace(
    session: &mut Session,
    id: &str,
    slot: &OnceLock<fn(&mut Session, &Value) -> CmdResult>,
    run: fn(&mut Session, &Value) -> CmdResult,
) {
    session.replace_command(id, |mut spec| {
        let _ = slot.set(spec.run);
        spec.run = run;
        spec
    });
}

/// Enrich core command param docs LawCraft relies on (prefix, style apply/basedOn).
fn enrich_param_docs(session: &mut Session) {
    session.replace_command("insert.pageNumber", |mut spec| {
        spec.params = concat!(
            r#"{"position"?: "top|bottom|current", "align"?: "left|center|right", "format"?: "x of y","#,
            r#" "prefix"?: string (text before the page number, e.g. appendix "A")}"#
        );
        spec
    });
    session.replace_command("styles.create", |mut spec| {
        spec.params = concat!(
            r#"{"name": string, "basedOn"?: string, "fromSelection"?: bool (default: true),"#,
            r#" "apply"?: bool (apply the new style to the selection; default: true)}"#
        );
        spec
    });
    session.replace_command("styles.modify", |mut spec| {
        spec.params = r#"{"style": string, "chr"?: CharProps, "para"?: ParaProps, "name"?: string, "basedOn"?: string, "next"?: string}"#;
        spec
    });
}

/// Wrap core commands LawCraft needs; safe to call on every [`crate::install`].
pub(crate) fn install_wraps(session: &mut Session) {
    capture_and_replace(session, "file.printPdf", &PRINT_PDF, wrap_print_pdf);
    capture_and_replace(session, "review.issues", &REVIEW_ISSUES, wrap_review_issues);
    enrich_param_docs(session);
}
