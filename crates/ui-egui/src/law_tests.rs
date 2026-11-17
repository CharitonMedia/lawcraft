//! LawCraft-owned UI tests: replaces_document save prompt and updater control behaviour.
use super::*;
use serde_json::{Value, json};
use wordcraft_engine::Session;

fn app() -> WordApp {
    WordApp::new(Session::new(wordcraft_doc::Document::new()), Services::default())
}

/// A module's command that opens a new document in place of this one asks to save unsaved
/// changes first, like File › New; a clean document goes straight through.
#[test]
fn a_command_that_replaces_the_document_asks_to_save_first() {
    fn replace(s: &mut Session, _: &Value) -> wordcraft_engine::CmdResult {
        s.set_document(wordcraft_doc::Document::new());
        Ok(json!({}))
    }
    let spec = || wordcraft_engine::CommandSpec::new("test.replace", "Replace", "Test", replace).replaces_document();
    let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), Services::default());
    app.session.install_commands(vec![spec()]);
    app.run("text.insert", json!({"text": "unsaved"})).unwrap();
    let r = app.run("test.replace", json!({})).unwrap();
    assert_eq!(r, json!({"pending": "saveChanges"}));
    assert!(app.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("unsaved"), "nothing is lost yet");
    app.run("ui.saveChanges", json!({"answer": "cancel"})).unwrap();
    assert!(app.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("unsaved"), "cancel keeps the work");
    app.session.dirty = false;
    app.run("test.replace", json!({})).unwrap();
    assert!(!app.session.doc.plain_text(wordcraft_doc::StoryRef::Body).contains("unsaved"), "a saved document is replaced at once");
}

#[test]
fn window_close_waits_for_update_worker() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use wordcraft_engine::update::LATEST_API;

    let hold = Arc::new(AtomicBool::new(true));
    let ctx = egui::Context::default();
    let mut a = app();
    a.ctx = Some(ctx.clone());
    let gate = Arc::clone(&hold);
    a.session.update_http = Some(Arc::new(move |u, _| {
        if u == LATEST_API {
            while gate.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(vec![])
    }));
    update::queue_check(&mut a, json!({"force": true}), true, &ctx);
    assert!(update::worker_busy(&a));
    assert!(a.close_requested(), "a plain check doesn't hold the window open");
    a.quit_requested = false;
    // Let the check finish, then start an install.
    hold.store(false, Ordering::Relaxed);
    for _ in 0..200 {
        update::poll(&mut a, &ctx);
        if !update::worker_busy(&a) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!update::worker_busy(&a));
    hold.store(true, Ordering::Relaxed);
    update::queue_check(&mut a, json!({"force": true, "apply": true}), false, &ctx);
    assert!(update::install_busy(&a));
    assert!(!a.close_requested(), "an install must not be interrupted by closing");
    hold.store(false, Ordering::Relaxed);
    for _ in 0..200 {
        update::poll(&mut a, &ctx);
        if !update::worker_busy(&a) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!update::worker_busy(&a));
    assert!(a.close_requested(), "close is allowed once the worker finishes");
}

#[test]
fn control_update_check_defers_until_worker_finishes() {
    use std::sync::Arc;
    use std::time::Duration;

    use wordcraft_engine::update::LATEST_API;

    let cur = env!("CARGO_PKG_VERSION");
    let latest = format!(r#"{{"tag_name":"v{cur}","draft":false,"prerelease":false,"assets":[]}}"#);
    let latest = Arc::new(latest);
    let ctx = egui::Context::default();
    let mut a = app();
    a.ctx = Some(ctx.clone());
    let body = Arc::clone(&latest);
    a.session.update_http = Some(Arc::new(move |u, _| if u == LATEST_API { Ok(body.as_bytes().to_vec()) } else { Ok(vec![]) }));
    let (req, rx) = ControlRequest::new("engine.execute", json!({"command": "file.checkForUpdates", "params": {"force": true}}));
    assert!(matches!(control::handle(&mut a, &ctx, &req), control::Outcome::Deferred));
    for _ in 0..200 {
        update::poll(&mut a, &ctx);
        if !update::worker_busy(&a) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!update::worker_busy(&a));
    let reply = rx.recv_timeout(Duration::from_secs(2)).expect("control reply");
    assert_eq!(reply.get("ok"), Some(&json!(true)));
    assert_eq!(reply.get("result").and_then(|r| r.get("status")).and_then(Value::as_str), Some("current"));
}
