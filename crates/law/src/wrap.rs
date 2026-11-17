//! Wrap core WordCraft commands so LawCraft module behaviour runs (print findings).

use std::sync::OnceLock;

use serde_json::Value;
use wordcraft_engine::{CmdError, CmdResult, Session};

/// Original `file.printPdf` run fn.
static PRINT_PDF: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();

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

/// Wrap module-facing commands; safe to call on every [`crate::install`].
pub(crate) fn install_wraps(session: &mut Session) {
    // review.issues lint merge and customizeRibbon add-in preservation live in
    // `wordcraft_engine::cmd::core_wraps` (always on). Only print findings stay law-only.
    if PRINT_PDF.get().is_none() {
        capture_and_replace(session, "file.printPdf", &PRINT_PDF, wrap_print_pdf);
    } else {
        session.replace_command("file.printPdf", |mut spec| {
            spec.run = wrap_print_pdf;
            spec
        });
    }
}
