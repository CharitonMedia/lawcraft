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

/// CLI help text (LawCraft lines included).
pub(crate) const USAGE: &str = "\
wordcraft-cli — WordCraft from the command line

USAGE:
  wordcraft-cli convert <in> <out> [--markup] [--update-fields] [--strict] [--format wordml2003|flatopc]
                                              convert between formats (docx, doc [read], pdf, odt, rtf, html, md, tex, txt, json, png,
                                              xml); --update-fields refreshes unlocked fields before saving
                                              (if protection blocks updates, they are skipped with a warning);
                                              an .xml file is a Word XML Document (Flat OPC), or with
                                              --format wordml2003 a Word 2003 XML Document;
                                              --markup (Print Markup): comments beside each page and
                                              tracked changes in pdf/png, a comment list in html/md/txt;
                                              --strict (pdf or png output only) writes nothing and exits 3
                                              if a character can't be drawn or a picture is missing or
                                              undecodable; the report is on stderr, with one JSON line
  wordcraft-cli info <file>                   pages, words, paragraphs, properties (JSON)
  wordcraft-cli text <file>                   plain text
  wordcraft-cli inspect <file>                document structure (JSON)
  wordcraft-cli render <file> <out.png> [--page N] [--scale S] [--strict]
                                              --strict writes nothing and exits 3 if a character on the
                                              page can't be drawn or a picture is missing or undecodable
                                              (report on stderr, as for convert --strict)
  wordcraft-cli run [--file F | --template T] [--script FILE|-] --cmd 'id={json}' [--cmd …]
                    [--save OUT] [--print] [--author NAME]
                                              run commands headlessly, then save (--author names
                                              who tracked changes and comments are by).
                                              --script runs a JSON file (- is stdin) first:
                                              {\"file\"?, \"template\"?, \"commands\": [{\"command\": id,
                                              \"params\"?: {…}} | id, …], \"save\"?: [OUT, …]}; save
                                              formats follow the extension (docx and pdf in one run);
                                              relative paths resolve against the script's folder.
                                              file.new and file.open open another document and
                                              make it active; later commands and the saves work on
                                              the active one; view.switchWindows lists the open
                                              documents and with {\"id\": N} makes one active
  wordcraft-cli commands [--json]             list every command
  wordcraft-cli parity [--markdown]           feature-catalog parity
  wordcraft-cli update --check                ask GitHub for a newer release (JSON)
  wordcraft-cli mcp [--connect HOST:PORT | --author NAME] [--automation-read-root DIR] [--automation-write-root DIR]
                                              MCP server on stdio (headless, or bridged to the app).
                                              --automation-read-root / --automation-write-root confine
                                              the files the agent's commands read (open, insert text
                                              or picture, mail-merge data, templates…) and write (save,
                                              export, merge output…) to DIR, links followed; a root left
                                              out grants none of its access. They apply to the headless
                                              server and are an error with --connect.
  wordcraft-cli zotero <command> <file> [--cmd 'id={json}' …] [--save OUT] [--trace] [--port P]
                                              run a Zotero command on a document (Zotero must be
                                              running): addEditCitation, addEditBibliography,
                                              addNote, refresh, removeCodes, setDocPrefs
                                              (--cmd runs first, e.g. --cmd caret.docEnd)
  wordcraft-cli [<command>] -h | --help       this help
  wordcraft-cli --version

A password-protected Word file is opened with --password PASS, --password-file PATH or the
WORDCRAFT_PASSWORD environment variable; a password given on the command line is readable by
every other process, so the other two are better. A file opened with a password is written with
it again, as the app does. Exports that cannot carry a password (pdf, html, txt) stay plain.

Paper: documents created here, and HTML, Markdown and text files (which give no page size), use
the system region's paper (A4, or Letter in the US and other Letter regions), read from
LC_ALL / LC_PAPER / LANG or the platform. Set WORDCRAFT_REGION=en-US (or another tag) for the
same output on every machine.

LawCraft (court profiles, `law.profiles.*`) is an opt-in module: its commands are there when the
app's options turn it on (law.enabled), or with WORDCRAFT_LAW=1 for one run. See docs/law.md.
";

/// Re-export findings after a successful export/run command result.
pub(crate) fn ok_findings(r: serde_json::Value) -> serde_json::Value {
    report_findings(&r);
    r
}

/// Report findings when `r` is `Ok` (convert's error path stays as `if let Err`).
pub(crate) fn report_if_ok<E>(r: &Result<serde_json::Value, E>) {
    if let Ok(v) = r {
        report_findings(v);
    }
}

