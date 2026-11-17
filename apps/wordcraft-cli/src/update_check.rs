//! HTTPS for `wordcraft-cli update --check` (#15).

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use wordcraft_engine::Session;
use wordcraft_engine::update::{HttpGet, LATEST_API};

/// The CLI's HTTP fetch for update checks: serves a local file instead of GitHub when
/// `WORDCRAFT_UPDATE_CHECK_FIXTURE` is set (tests), else fetches over the network.
pub fn http_get() -> HttpGet {
    if let Ok(path) = std::env::var("WORDCRAFT_UPDATE_CHECK_FIXTURE") {
        let body = std::fs::read(&path).map_err(|e| e.to_string());
        return Arc::new(move |url, max_bytes| {
            if url != LATEST_API {
                return Err(format!("unexpected URL: {url}"));
            }
            let body = body.as_ref().map_err(|e| e.clone())?;
            let end = body.len().min(max_bytes);
            Ok(body[..end].to_vec())
        });
    }
    Arc::new(|url, max_bytes| {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(concat!("wordcraft-cli/", env!("CARGO_PKG_VERSION"), " (+https://github.com/CharitonMedia/lawcraft)"))
            .build()
            .into();
        let mut response = agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| e.to_string())?;
        response.body_mut().with_config().limit(max_bytes as u64).read_to_vec().map_err(|e| e.to_string())
    })
}

/// Run `file.checkForUpdates` for `wordcraft-cli update --check` and return the result. The HTTP
/// hook and the install-detection environment are injected so tests run offline and independent of
/// the host; the response must stay a single JSON value on stdout, with no probe output leaking in.
pub(crate) fn run_update_check(http: HttpGet, env: Option<wordcraft_engine::update::EnvReadBox>) -> Result<Value, String> {
    let mut s = Session::new(wordcraft_doc::Document::new());
    s.update_http = Some(http);
    s.update_env = env;
    // A test hook (`WORDCRAFT_UPDATE_CHECK_EXE`) lets the subprocess test put the running path
    // under `/usr/bin` so install detection actually probes `dpkg-query`; otherwise the CLI's own
    // path is used.
    if let Ok(exe) = std::env::var("WORDCRAFT_UPDATE_CHECK_EXE") {
        s.update_exe = Some(exe);
    } else if let Ok(exe) = std::env::current_exe() {
        s.update_exe = Some(exe.to_string_lossy().into_owned());
    }
    s.run("file.checkForUpdates", &serde_json::json!({"force": true})).map_err(|e| e.to_string())
}

/// Pretty JSON for `wordcraft-cli update --check` (one value, trailing newline).
pub(crate) fn format_update_check_json(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}
