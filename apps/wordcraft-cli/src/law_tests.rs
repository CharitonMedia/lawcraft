//! LawCraft-owned CLI tests: update check offline, JSON shape, and PDF open/save.
use super::*;
use crate::update_check::{format_update_check_json, run_update_check};
use serde_json::{Value, json};
use std::path::Path;

/// #15: the headless MCP backend must carry the update HTTP hook, so `file.checkForUpdates`
/// works through MCP. The injected fake proves it with no real request.
#[test]
fn headless_mcp_checks_for_updates_offline() {
    use std::sync::{Arc, Mutex};
    let calls = Arc::new(Mutex::new(Vec::new()));
    let c = Arc::clone(&calls);
    let http: wordcraft_engine::update::HttpGet = Arc::new(move |url, _max| {
        c.lock().unwrap_or_else(|e| e.into_inner()).push(url.to_string());
        Ok(br#"{"tag_name":"v0.0.1","draft":false,"prerelease":false,"assets":[]}"#.to_vec())
    });
    let mut h = headless_session(http, None, None).expect("headless backend");
    assert!(h.session.update_http.is_some(), "the hook is installed");
    let r = wordcraft_mcp::Backend::call(&mut h, "engine.execute", json!({"command": "file.checkForUpdates", "params": {"force": true}}))
        .expect("mcp call");
    assert_eq!(r["status"], "current");
    let seen = calls.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(seen, vec![wordcraft_engine::update::LATEST_API.to_string()], "exactly one offline release request");
}

/// #15: `wordcraft-cli update --check` prints exactly one JSON value. Install detection runs
/// inside the command, so this pins the response shape at the CLI boundary: `serde_json` rejects
/// any probe output that precedes or follows the value. The engine's captured probe helper is
/// what keeps a `dpkg-query -S` from inheriting stdout in the first place.
#[test]
fn update_check_prints_one_json_value() {
    struct FakeEnv;
    impl wordcraft_engine::update::EnvRead for FakeEnv {
        fn flatpak(&self) -> bool {
            false
        }
        fn appimage(&self) -> Option<String> {
            None
        }
        fn install_override(&self) -> Option<wordcraft_engine::update::InstallKind> {
            Some(wordcraft_engine::update::InstallKind::Unknown)
        }
        fn windows(&self) -> bool {
            false
        }
        fn macos(&self) -> bool {
            false
        }
        fn linux(&self) -> bool {
            true
        }
        fn linux_deb_package(&self, _exe: &str) -> bool {
            false
        }
    }
    let http: wordcraft_engine::update::HttpGet =
        std::sync::Arc::new(|_, _| Ok(br#"{"tag_name":"v0.0.1","draft":false,"prerelease":false,"assets":[]}"#.to_vec()));
    let v = run_update_check(http, Some(std::sync::Arc::new(FakeEnv))).expect("offline check");
    let text = format_update_check_json(&v);
    let _: Value = serde_json::from_str(text.trim()).expect("the response is one JSON value");
    assert!(text.trim_start().starts_with('{') && text.trim_end().ends_with('}'), "no probe output wraps the JSON: {text:?}");
}

#[test]
fn cli_open_does_not_set_the_pdf_as_an_implicit_save_target() {
    let path = std::env::temp_dir().join(format!("wordcraft-cli-pdf-import-{}.pdf", std::process::id()));
    let bytes = wordcraft_engine::io::save_bytes("sample.pdf", &wordcraft_doc::Document::from_text("CLI import")).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut session = open(Path::new(&path), None).unwrap();
    assert!(session.path.is_none() && session.dirty);
    let result = session.run("file.save", &json!({})).unwrap();
    assert_eq!(result["saved"], false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}
