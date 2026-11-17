//! Headless session construction shared by CLI subcommands and MCP.

use wordcraft_engine::Session;

use crate::update_check;

/// A headless session in the system region: every way the CLI starts one gives `file.new` the
/// same paper (#1381).
pub(crate) fn session(doc: wordcraft_doc::Document) -> Session {
    let mut s = Session::new(doc);
    // `run --cmd file.checkForUpdates` reaches the release feed like `update --check` does.
    s.update_http = Some(update_check::http_get());
    s.system_region = super::system_region();
    s
}

/// Build the headless MCP session, wiring the same update HTTP hook the desktop app uses so
/// `file.checkForUpdates` is reachable through MCP too (#15). `http` is injected so tests run with
/// an offline fake and no request ever leaves the machine.
pub(crate) fn headless_session(
    http: wordcraft_engine::update::HttpGet,
    author: Option<String>,
    roots: Option<std::sync::Arc<wordcraft_engine::file_access::AutomationRoots>>,
) -> Result<wordcraft_mcp::Headless, String> {
    let mut headless = wordcraft_mcp::Headless::default();
    headless.session.update_http = Some(http);
    if let Some(roots) = roots {
        // Every thread of this process, not only the server's: nothing reaches past the roots.
        wordcraft_engine::file_access::confine_process(roots.clone());
        let show = |root: Option<&std::path::Path>| root.map_or_else(|| "none".to_string(), |r| r.display().to_string());
        eprintln!("wordcraft-cli: files confined: read root {}, write root {}", show(roots.read_root()), show(roots.write_root()));
        headless = headless.with_automation_roots(Some(roots));
    }
    if let Some(name) = author {
        super::set_author(&mut headless.session, &name)?;
    }
    Ok(headless)
}
