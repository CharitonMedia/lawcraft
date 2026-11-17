//! LawCraft module wiring for the CLI: opt-in install, export findings, PDF open handling.

use std::path::Path;

use wordcraft_engine::Session;

/// Whether the LawCraft module (`law.*` commands) is on: `WORDCRAFT_LAW` for one run (`0` turns it
/// off), else the user's setting (`modules.law` in the saved preferences, the one the app writes).
/// On by default in LawCraft.
pub(crate) fn law_enabled() -> bool {
    if let Some(v) = std::env::var_os("WORDCRAFT_LAW") {
        return !v.is_empty() && v != "0";
    }
    if std::env::var_os("WORDCRAFT_NO_PREFS").is_some() {
        return true;
    }
    let Some(dir) = wordcraft_control_key::settings_dir() else { return true };
    let Ok(bytes) = std::fs::read(dir.join("ui.json")) else { return true };
    let Ok(saved) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return true };
    saved.pointer("/editing/modules/law").and_then(serde_json::Value::as_bool) != Some(false)
}

/// Turn the LawCraft module on for a headless session, when it is on.
pub(crate) fn with_law(mut s: Session) -> Session {
    enable_law(&mut s);
    s
}

/// [`with_law`] for a session that already exists (the MCP bridge's headless backend).
pub(crate) fn enable_law(s: &mut Session) {
    if law_enabled() {
        // Runs without preferences (`WORDCRAFT_NO_PREFS`) keep profiles in memory only.
        let dir = if std::env::var_os("WORDCRAFT_NO_PREFS").is_some() { None } else { wordcraft_law::settings_dir() };
        wordcraft_law::set_profiles_dir(dir);
        // A headless session keeps no saved preferences, so the flag or the setting decides here.
        s.prefs.modules.insert(wordcraft_law::MODULE.into(), true);
        wordcraft_law::install(s);
    }
}

/// Print an export's check findings (an applied court profile's, say) to standard error, one per
/// line; the export itself still went through.
pub(crate) fn report_findings(r: &serde_json::Value) {
    for f in r.get("findings").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let severity = f.get("severity").and_then(serde_json::Value::as_str).unwrap_or("note");
        let message = f.get("message").and_then(serde_json::Value::as_str).unwrap_or("");
        eprintln!("{severity}: {message}");
    }
}

/// Finish opening a path after [`wordcraft_engine::io::open_path_report_in`]: a PDF becomes a new
/// dirty document with warnings on stderr and no save target; a Word file keeps its path and
/// password as before. `pdf_warnings` is `Some` when the open was a PDF import.
pub(crate) fn apply_opened(s: &mut Session, path: &Path, password: Option<&str>, encrypted: bool, pdf_warnings: Option<Vec<String>>) {
    if let Some(warnings) = pdf_warnings {
        for warning in warnings {
            eprintln!("PDF import: {warning}");
        }
        s.dirty = true;
    } else {
        s.path = Some(path.into());
        if encrypted {
            s.password = password.map(wordcraft_engine::io::Password::new);
        }
    }
}
