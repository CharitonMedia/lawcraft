//! Wrap core WordCraft commands so LawCraft module behaviour runs (print findings, ribbon add-ins).

use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};
use wordcraft_engine::{CmdError, CmdResult, Session};

/// Original `file.printPdf` run fn.
static PRINT_PDF: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
/// Original `review.issues` run fn.
static REVIEW_ISSUES: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();
static CUSTOMIZE_RIBBON: OnceLock<fn(&mut Session, &Value) -> CmdResult> = OnceLock::new();

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

fn wrap_customize_ribbon(s: &mut Session, v: &Value) -> CmdResult {
    let Some(orig) = CUSTOMIZE_RIBBON.get() else {
        return Err(CmdError::Failed("tools.customizeRibbon wrap missing".into()));
    };
    let mut v2 = v.clone();
    let mut handled = false;
    if let Some(x) = v.get("layout") {
        s.ribbon.replace_layout(wordcraft_engine::RibbonLayout::from_value(x));
        let reg = Arc::clone(&s.registry);
        s.ribbon.retain_known(&reg);
        if let Some(obj) = v2.as_object_mut() {
            obj.remove("layout");
        }
        handled = true;
    }
    if v.get("reset").and_then(Value::as_bool) == Some(true) {
        s.ribbon.reset_ribbon();
        s.ribbon.reset_qat();
        if let Some(obj) = v2.as_object_mut() {
            obj.remove("reset");
        }
        handled = true;
    }
    let remaining = v2.as_object().is_some_and(|o| !o.is_empty());
    if remaining {
        return orig(s, &v2);
    }
    if handled {
        return Ok(json!({
            "tabs": serde_json::to_value(s.ribbon.tabs().as_ref()).unwrap_or(Value::Null),
            "qat": s.ribbon.qat(),
        }));
    }
    orig(s, v)
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
    capture_and_replace(session, "file.printPdf", &PRINT_PDF, wrap_print_pdf);
    capture_and_replace(session, "review.issues", &REVIEW_ISSUES, wrap_review_issues);
    capture_and_replace(session, "tools.customizeRibbon", &CUSTOMIZE_RIBBON, wrap_customize_ribbon);
}
