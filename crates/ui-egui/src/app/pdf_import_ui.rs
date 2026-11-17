//! PDF import report dialog opened from an engine UI request.

use serde_json::Value;

use super::{WordApp, dialogs};

/// Open the PDF import warnings dialog from a `{"open":"pdfImportReport","warnings":[…]}` request.
pub(crate) fn open_report(app: &mut WordApp, req: &Value) {
    let warnings = req.get("warnings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
    app.dialog = Some(dialogs::Dialog::PdfImportReport { warnings });
}
