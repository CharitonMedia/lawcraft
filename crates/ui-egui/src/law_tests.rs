//! LawCraft-owned UI tests: replaces_document save prompt, PDF import, and updater behaviour.
use super::*;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};
use wordcraft_engine::Session;

fn app() -> WordApp {
    WordApp::new(Session::new(wordcraft_doc::Document::new()), Services::default())
}

/// An app with one comment ("Original") on some text, its balloon on screen.
fn comment_harness() -> (egui_kittest::Harness<'static, WordApp>, u32) {
    let mut a = app();
    a.run("text.insert", json!({"text": "Some text here"})).unwrap();
    a.run("select.text", json!({"text": "text"})).unwrap();
    let id = a.run("review.newComment", json!({"text": "Original"})).unwrap()["id"].as_u64().unwrap() as u32;
    a.run("view.commentsPane", json!({"value": false})).unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        a,
    );
    for _ in 0..6 {
        h.step();
    }
    (h, id)
}

fn click_balloon(h: &mut egui_kittest::Harness<'static, WordApp>, id: u32) {
    h.state_mut().canvas.balloon = None;
    for _ in 0..2 {
        h.step();
    }
    let card = h.state().canvas.balloon_rects.iter().find(|(i, _)| *i == id).map(|(_, r)| *r).expect("the comment has a balloon");
    let at = card.left_top() + egui::vec2(30.0, 8.0);
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.step();
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(h.state().canvas.balloon, Some(id), "the click selected the balloon");
}

fn type_in(h: &mut egui_kittest::Harness<'static, WordApp>, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.step();
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

/// #15 review: a manual check that finds nothing new, or cannot reach GitHub, must say so; only
/// an available update draws its own notice. Backstage's status bar is hidden, so the answer is
/// rendered in the update bar too (asserted below).
#[test]
fn manual_update_check_reports_current_error_and_failure() {
    let mut a = app();
    update::report_update_check(&mut a, Ok(json!({"status": "current", "current": "0.6.1"})));
    assert!(a.status_msg.as_ref().is_some_and(|(m, _)| m.contains("up to date")), "{:?}", a.status_msg);
    update::report_update_check(
        &mut a,
        Ok(json!({"status": "error", "error": wordcraft_engine::update::MSG_CHECK_FAILED, "errorDetail": "dns"})),
    );
    assert!(a.status_msg.as_ref().is_some_and(|(m, _)| m.contains("reach GitHub")), "{:?}", a.status_msg);
    update::report_update_check(&mut a, Err("nope".to_string()));
    assert!(a.status_msg.as_ref().is_some_and(|(m, _)| m.contains("reach GitHub")), "{:?}", a.status_msg);
    a.status_msg = None;
    update::report_update_check(&mut a, Ok(json!({"status": "available"})));
    assert!(a.status_msg.is_none(), "the notice bar speaks for itself: {:?}", a.status_msg);
    assert!(a.update.message.is_none(), "an available update draws its own notice, not the bar message");
}

/// #15 review: File ▸ Check for updates runs from Backstage, whose status bar is hidden, so the
/// "up to date" answer must render in the update bar there.
#[test]
fn manual_update_check_answer_is_rendered_in_backstage() {
    let mut a = app();
    a.run("ui.backstage", json!({"value": true, "page": "home"})).unwrap();
    update::report_update_check(&mut a, Ok(json!({"status": "current", "current": "0.6.1"})));
    assert!(a.update.message.as_deref().is_some_and(|m| m.contains("up to date")), "{:?}", a.update.message);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut WordApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        a,
    );
    h.run_steps(4);
    assert!(h.query_by_label("WordCraft is up to date.").is_some(), "the answer is rendered over Backstage");
}

/// #1348: a module's command that replaces the document (New Pleading, say) counts comment
/// text still being typed as an unsaved change and asks first.
#[test]
fn a_command_that_replaces_the_document_asks_about_a_pending_comment_draft() {
    fn replace(s: &mut Session, _: &Value) -> wordcraft_engine::CmdResult {
        s.set_document(wordcraft_doc::Document::new());
        Ok(json!({}))
    }
    let (mut h, id) = comment_harness();
    h.state_mut()
        .session
        .install_commands(vec![wordcraft_engine::CommandSpec::new("test.replace", "Replace", "Test", replace).replaces_document()]);
    h.state_mut().session.dirty = false;
    click_balloon(&mut h, id);
    type_in(&mut h, "Typed ");
    let document = h.state().session.document_id();
    assert_eq!(h.state_mut().run("test.replace", json!({})).unwrap(), json!({"pending": "saveChanges"}));
    assert_eq!(h.state().session.document_id(), document, "nothing is replaced yet");
}

#[test]
fn pdf_import_picker_and_report_keep_the_conversion_unsaved() {
    assert!(file_dialogs::open_filters("document")[0].1.contains(&"pdf"));
    let mut app = app();
    let bytes = wordcraft_engine::io::save_bytes("original.pdf", &wordcraft_doc::Document::from_text("Editable in the window")).unwrap();
    app.run("file.open", json!({"path": "original.pdf", "data": wordcraft_engine::cmd::insert::base64_encode(&bytes)}))
        .unwrap();
    let body = app.session.doc.plain_text(wordcraft_doc::StoryRef::Body);
    assert!(body.contains("Editable in the window"));
    assert!(app.session.dirty && app.session.path.is_none());
    assert!(matches!(&app.dialog, Some(dialogs::Dialog::PdfImportReport { warnings }) if !warnings.is_empty()));
    app.dialog = None;
    app.run("text.insert", json!({"text": "Edited: "})).unwrap();
    assert!(app.session.doc.plain_text(wordcraft_doc::StoryRef::Body).starts_with("Edited: "));
}
