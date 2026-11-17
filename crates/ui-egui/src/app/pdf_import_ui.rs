//! PDF import report shown after opening a PDF as an editable copy.

use egui::Ui;
use serde_json::Value;

use super::WordApp;

/// Pending PDF import warnings to show once (not a [`crate::dialogs::Dialog`] variant).
#[derive(Default)]
pub(crate) struct PdfImportReport {
    pub warnings: Vec<String>,
}

/// Open the PDF import warnings report from a `{"open":"pdfImportReport","warnings":[…]}` request.
pub(crate) fn open_report(app: &mut WordApp, req: &Value) {
    let warnings = req.get("warnings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    app.pdf_import_report = Some(PdfImportReport { warnings });
}

/// Draw the report window when one is open; clears it when dismissed.
pub(crate) fn show(app: &mut WordApp, ctx: &egui::Context) {
    let Some(report) = app.pdf_import_report.as_ref() else {
        return;
    };
    let warnings = report.warnings.clone();
    let mut open = true;
    let mut done = false;
    egui::Window::new(tl!("PDF import"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -40.0))
        .open(&mut open)
        .show(ctx, |ui| {
            done = body(ui, &warnings);
        });
    if done || !open {
        app.pdf_import_report = None;
    }
}

fn body(ui: &mut Ui, warnings: &[String]) -> bool {
    ui.set_max_width(460.0);
    ui.label(tl!("The PDF is now an editable copy. Save it as a new document; the original PDF has not changed."));
    ui.add_space(8.0);
    for warning in warnings {
        ui.label(tl!(warning.as_str()));
    }
    ui.add_space(8.0);
    ui.button(tl!("OK")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape))
}
