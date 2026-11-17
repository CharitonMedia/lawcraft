//! Optional-module session APIs (install/uninstall, lint findings, export checks).
//!
//! Kept out of WordCraft's [`super::lifecycle`] so LawCraft additions stay in one place.

use super::{Arc, Session};
use crate::CommandSpec;

impl Session {
    /// Whether an optional module is turned on ([`super::Prefs::modules`], `law` for LawCraft): a
    /// module is off until the user switches it on, and a host installs its commands only then.
    pub fn module_enabled(&self, id: &str) -> bool {
        self.prefs.module_enabled(id)
    }

    /// Insert a generated list field (TOC / TOA / bibliography / index) via the core helper.
    /// LawCraft wraps `references.tableOfAuthorities` and calls this with TOA switches.
    pub fn insert_generated_list(&mut self, instr: &str, title: &str) -> Result<(), crate::CmdError> {
        crate::cmd::citations::generated_list(self, instr, title)
    }

    /// Set English module supplements for proofing (layout Review commands and on-screen squiggles).
    ///
    /// Only stores the value on this session. Layout and Review proofing install it for the
    /// duration of their work via [`wordcraft_proof::enter_session_module`], so another session
    /// created on the same thread cannot clear or replace this one's module.
    pub fn set_module_en(&mut self, module: wordcraft_proof::ModuleEn) {
        self.module_en = module;
    }

    /// Run `f` with this session's [`Self::module_en`] installed for proofing on this thread.
    pub fn with_proof_module<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = wordcraft_proof::enter_session_module(self.module_en);
        f()
    }

    /// Install commands an optional module supplies (`law.profiles.apply` from LawCraft, say).
    /// The host calls this once at start-up, when the module is turned on, and every way in
    /// reaches them from then on: the ribbon, the command palette, shortcuts, the CLI, the JSON
    /// control channel and the MCP server all dispatch by id. A command whose id the core
    /// already has is skipped, so a module can never shadow core behaviour. Returns how many
    /// commands were added.
    ///
    /// Nothing here knows what any module does: the engine holds the specs and the switch, the
    /// module brings the behaviour.
    pub fn install_commands(&mut self, specs: Vec<CommandSpec>) -> usize {
        let mut all = self.registry.all().to_vec();
        let mut added = 0;
        for spec in specs {
            if self.registry.get(spec.id).is_some() {
                log::warn!("command `{}` is already registered; the module's is ignored", spec.id);
                continue;
            }
            all.push(spec);
            added += 1;
        }
        self.registry = Arc::new(crate::Registry::new(all));
        added
    }

    /// Replace a registered command in place (an optional module may wrap a core command). The
    /// replacement must keep the same id. Returns whether a command was replaced.
    pub fn replace_command(&mut self, id: &str, map: impl FnOnce(CommandSpec) -> CommandSpec) -> bool {
        let mut all = self.registry.all().to_vec();
        let Some(i) = all.iter().position(|c| c.id == id) else {
            return false;
        };
        let replacement = map(all[i].clone());
        if replacement.id != id {
            log::warn!("replace_command: id must stay `{id}`, got `{}`", replacement.id);
            return false;
        }
        all[i] = replacement;
        self.registry = Arc::new(crate::Registry::new(all));
        true
    }

    /// Document-level lint findings for Review › Proofing (`review.issues`): every source's, in
    /// source order, at most [`crate::lint::MAX_FINDINGS`].
    pub fn lint_findings(&self) -> Vec<serde_json::Value> {
        self.lint_findings.values().flatten().take(crate::lint::MAX_FINDINGS).cloned().collect()
    }

    /// The findings one source stored last.
    pub fn lint_findings_of(&self, source: &str) -> &[serde_json::Value] {
        self.lint_findings.get(source).map_or(&[], Vec::as_slice)
    }

    /// Replace one source's document-level lint findings (a module's check command calls this
    /// with its own `source`, so separate checks don't overwrite each other).
    pub fn set_lint_findings(&mut self, source: &str, findings: Vec<crate::lint::Finding>) {
        let json: Vec<serde_json::Value> =
            findings.into_iter().take(crate::lint::MAX_FINDINGS).map(|f| crate::lint::finding_json(&f, crate::cmd::pos_json)).collect();
        if json.is_empty() {
            self.lint_findings.remove(source);
        } else {
            self.lint_findings.insert(source.to_string(), json);
        }
    }

    /// Forget every source whose name starts with `prefix` (a module turned off).
    pub fn clear_lint_findings(&mut self, prefix: &str) {
        self.lint_findings.retain(|k, _| !k.starts_with(prefix));
    }

    /// Keep only the export checks whose command id passes `keep` (a module turned off drops its own).
    pub fn retain_export_check_commands(&mut self, keep: impl Fn(&str) -> bool) {
        self.export_check_commands.retain(|(id, _)| keep(id));
    }

    /// Remove module commands by id (a module turned off). Core commands are never removed: a
    /// module can't have installed one ([`Session::install_commands`] skips them). Returns how
    /// many were removed.
    pub fn uninstall_commands(&mut self, ids: &[&str]) -> usize {
        let core = crate::cmd::registry();
        let all: Vec<CommandSpec> = self.registry.all().iter().filter(|c| !ids.contains(&c.id) || core.get(c.id).is_some()).cloned().collect();
        let removed = self.registry.all().len() - all.len();
        if removed > 0 {
            self.registry = Arc::new(crate::Registry::new(all));
        }
        removed
    }

    /// Commands run before a save or export; each command's result may include a `findings` array.
    pub fn set_export_check_commands(&mut self, commands: Vec<(String, serde_json::Value)>) {
        self.export_check_commands = commands;
    }

    /// Run the session's export-check commands with the export's own options and target `format` (a
    /// lower-case extension such as `pdf`, `docx`, `png`) and return their findings. Each check
    /// stores its own findings; a check that fails outright is stored as a finding of its own source.
    /// Each command's parameters carry the actual `markup` option and the output `format`, so a
    /// check that re-exports the document (such as `textSearchablePdf`) verifies what is being
    /// written and only the checks that apply to that format run.
    pub fn run_export_checks(&mut self, opts: &crate::io::ExportOptions, format: &str) -> Vec<serde_json::Value> {
        let cmds = self.export_check_commands.clone();
        let mut all = Vec::new();
        for (id, params) in cmds {
            let mut params = params;
            if let Some(obj) = params.as_object_mut() {
                obj.insert("markup".into(), serde_json::json!(opts.markup));
                obj.insert("format".into(), serde_json::json!(format));
            }
            // A check that may change the document (a word count kept current) and finds nothing
            // to change leaves no undo step and doesn't mark the document changed.
            let mutates = self.registry.get(&id).is_some_and(|c| c.mutates);
            let kept = mutates.then(|| {
                (
                    self.doc.clone(),
                    self.history.clone(),
                    self.redo.clone(),
                    self.dirty,
                    self.rev,
                    self.typing_open,
                    self.undo_evicted,
                    self.edit_marks.clone(),
                )
            });
            // An export runs these on its own: they aren't steps of a macro being recorded, and
            // Repeat still repeats what the person last did.
            let recording = self.recording.as_ref().map(|(_, steps)| steps.len());
            let repeat = self.last_command.clone();
            let result = self.run(&id, &params);
            if let (Some(n), Some((_, steps))) = (recording, self.recording.as_mut()) {
                steps.truncate(n);
            }
            self.last_command = repeat;
            match result {
                Ok(v) => {
                    if let Some((doc, history, redo, dirty, rev, typing_open, undo_evicted, edit_marks)) = kept
                        && self.doc == doc
                    {
                        self.history = history;
                        self.redo = redo;
                        self.dirty = dirty;
                        self.rev = rev;
                        self.typing_open = typing_open;
                        self.undo_evicted = undo_evicted;
                        self.edit_marks = edit_marks;
                    }
                    // A check that failed on an earlier export and runs now drops that failure.
                    self.set_lint_findings(&format!("export:{id}"), Vec::new());
                    if let Some(arr) = v.get("findings").and_then(serde_json::Value::as_array) {
                        for f in arr {
                            all.push(f.clone());
                            if all.len() >= crate::lint::MAX_FINDINGS {
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    // The same shape as any other lint finding, so Review › Proofing and
                    // `review.issues` can show a failed check beside the rest.
                    let finding = crate::lint::Finding::error(id.clone(), e.to_string());
                    all.push(crate::lint::finding_json(&finding, crate::cmd::pos_json));
                    self.set_lint_findings(&format!("export:{id}"), vec![finding]);
                    if all.len() >= crate::lint::MAX_FINDINGS {
                        break;
                    }
                }
            }
            if all.len() >= crate::lint::MAX_FINDINGS {
                break;
            }
        }
        all
    }
}

/// LawCraft fields for a new session: lint map from the document state, empty export checks, and
/// default module English supplements. Does not touch the proofing thread-local — that is scoped
/// per layout / Review call from the session that owns the module.
pub(super) fn init_fields(
    lint_findings: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
) -> (std::collections::BTreeMap<String, Vec<serde_json::Value>>, Vec<(String, serde_json::Value)>, wordcraft_proof::ModuleEn) {
    (lint_findings, Vec::new(), wordcraft_proof::ModuleEn::default())
}
