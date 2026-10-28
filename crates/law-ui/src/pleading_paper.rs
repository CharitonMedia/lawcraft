//! File › Options › LawCraft › a court profile › Pleading Paper: the document type's
//! `pleadingPaper` block, field by field. English only, like the rest of the page.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::Ctx;
use crate::page::{commit_pending_drafts, reload_profile, save_doc};

/// The defaults `law.pleadingPaper.apply` uses, for a block being switched on.
fn defaults() -> Value {
    json!({"enabled": true, "lines": 28, "spacing": 24, "start": 0, "numberSize": 12, "numberGap": 8, "leftRule": "double", "rightRule": "single", "ruleGap": 4, "ruleWidth": 0.75})
}

/// The Pleading Paper section for one document type of a profile.
pub fn editor(app: &mut Ctx<'_>, ui: &mut Ui, profile_id: &str, doc_key: &str, doc: &Value, profile: &mut Value) {
    let block = doc.get("pleadingPaper").filter(|v| v.is_object()).cloned();
    let mut on = block.as_ref().is_some_and(|b| b["enabled"].as_bool().unwrap_or(true));
    ui.label(
        RichText::new("Numbered lines down the left margin of every page, with rules beside the text (California Rules of Court 2.108).")
            .small()
            .weak(),
    );
    if ui.checkbox(&mut on, "Use pleading paper").changed() && commit_pending_drafts(app) {
        reload_profile(app, profile_id, profile);
        let mut b = block.clone().unwrap_or_else(defaults);
        b["enabled"] = json!(on);
        // Word-style line numbering and pleading paper would number the page twice.
        let mut patch = json!({"pleadingPaper": b});
        if on {
            patch["lineNumbers"] = json!({"enabled": false});
        }
        save_doc(app, profile_id, doc_key, profile, patch);
        return;
    }
    let Some(b) = block.filter(|_| on) else { return };
    let num = |key: &str, d: f64| b[key].as_f64().unwrap_or(d);

    let mut lines = num("lines", 28.0) as u32;
    let mut spacing = num("spacing", 24.0) as f32;
    let mut start = num("start", 0.0) as f32;
    let mut size = num("numberSize", 12.0) as f32;
    let mut gap = num("numberGap", 8.0) as f32;
    let mut rule_gap = num("ruleGap", 4.0) as f32;
    let mut rule_w = num("ruleWidth", 0.75) as f32;
    let mut changed: Option<(&str, Value)> = None;
    egui::Grid::new(("law_pleading_paper", profile_id, doc_key)).num_columns(2).show(ui, |ui| {
        let l = ui.label("Lines per page:");
        if ui.add(egui::DragValue::new(&mut lines).range(10..=50)).labelled_by(l.id).changed() {
            changed = Some(("lines", json!(lines)));
        }
        ui.end_row();
        let l = ui.label("Line spacing (pt):");
        if ui.add(egui::DragValue::new(&mut spacing).speed(0.5).range(12.0..=36.0)).labelled_by(l.id).changed() {
            changed = Some(("spacing", json!(spacing)));
        }
        ui.end_row();
        let l = ui.label("First number below top margin (pt):");
        if ui.add(egui::DragValue::new(&mut start).speed(0.5).range(-72.0..=144.0)).labelled_by(l.id).changed() {
            changed = Some(("start", json!(start)));
        }
        ui.end_row();
        let l = ui.label("Number size (pt):");
        if ui.add(egui::DragValue::new(&mut size).speed(0.5).range(6.0..=72.0)).labelled_by(l.id).changed() {
            changed = Some(("numberSize", json!(size)));
        }
        ui.end_row();
        let l = ui.label("Gap before the left rule (pt):");
        if ui.add(egui::DragValue::new(&mut gap).speed(0.5).range(0.0..=72.0)).labelled_by(l.id).changed() {
            changed = Some(("numberGap", json!(gap)));
        }
        ui.end_row();
        for (key, label, options) in
            [("leftRule", "Left rule:", &["double", "single", "none"][..]), ("rightRule", "Right rule:", &["single", "none"][..])]
        {
            let l = ui.label(label);
            let cur = b[key].as_str().unwrap_or(options[0]).to_string();
            let mut pick = cur.clone();
            egui::ComboBox::from_id_salt(("law_pp_rule", key, profile_id, doc_key))
                .selected_text(&pick)
                .show_ui(ui, |ui| {
                    for o in options {
                        ui.selectable_value(&mut pick, (*o).to_string(), *o);
                    }
                })
                .response
                .labelled_by(l.id);
            if pick != cur {
                changed = Some((key, json!(pick)));
            }
            ui.end_row();
        }
        let l = ui.label("Gap between rules and text (pt):");
        if ui.add(egui::DragValue::new(&mut rule_gap).speed(0.5).range(0.0..=72.0)).labelled_by(l.id).changed() {
            changed = Some(("ruleGap", json!(rule_gap)));
        }
        ui.end_row();
        let l = ui.label("Rule weight (pt):");
        if ui.add(egui::DragValue::new(&mut rule_w).speed(0.05).range(0.25..=6.0)).labelled_by(l.id).changed() {
            changed = Some(("ruleWidth", json!(rule_w)));
        }
        ui.end_row();
    });
    if let Some((key, v)) = changed
        && commit_pending_drafts(app)
    {
        reload_profile(app, profile_id, profile);
        let mut nb = b.clone();
        nb[key] = v;
        save_doc(app, profile_id, doc_key, profile, json!({"pleadingPaper": nb}));
    }
}
