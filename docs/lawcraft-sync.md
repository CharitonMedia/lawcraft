# LawCraft and WordCraft: what LawCraft changes in WordCraft's files

LawCraft is WordCraft plus the legal add-in (`crates/law`, `crates/law-ui`, `crates/law-docx`)
and a thin proof facade (`crates/proof`). Once a week `.github/sync/sync.sh` merges WordCraft's
**newest release tag** (highest `vX.Y.Z`). LawCraft's own crates never conflict; the conflicts
come from the lines LawCraft changes inside WordCraft's files. This page lists every such file,
why LawCraft changes it, and what happens when a merge conflicts there.

A file **not on this list** must have no LawCraft edits. `sync.sh` fails the sync when a
conflicting path is missing from the list below (backtick-wrapped paths and `assets/app-icon/*`).

**Fixed by the sync on its own** means `sync.sh` resolves the conflict and the sync's gate still
has to pass before anything is pushed. **By hand** means the sync stops, opens the "Weekly
WordCraft sync needs attention" issue and assigns it; the fix is to take WordCraft's version and
add LawCraft's lines back where WordCraft now keeps that code.

Last checked against WordCraft `v0.18.1` (`079f478b`), 2026-11-17 (Pass 3).
**42** modified WordCraft code files under `crates/` and `apps/` (Pass 2 was 39; target under 40).
Three unavoidable Pass 3 additions are listed below; each is under 30 added lines.
No such file has more than 30 added lines vs `v0.18.1`. Branding, packaging, docs, and
workspace files outside that count may still differ; they are listed below.

### Pass 3 unavoidable WordCraft-file edits (+3)

| File | Why it had to change | Lines |
|---|---|---|
| `crates/engine/src/session/layout.rs` | Scope `module_en` with `enter_session_module` around squiggle/layout proofing so a second `Session::new` on the same thread cannot clear LawCraft session A's module (thread-local bleed). Also copy `module_en` into `preview_layout`. | +3 |

LawCraft-owned **new** modules (not WordCraft files; they do not appear in
`--diff-filter=M`) hold the bulk: `crates/engine/src/{hooks,lint,update,law_tests,tests_hooks,
ribbon_addins,file_access_automation,io_open}.rs`, `session/modules.rs`,
`cmd/{tracked_edit,file_open,core_wraps,update,citations/toa_law}.rs`,
`crates/pdf/src/{extract,import,searchability,law_tests}.rs`,
`crates/ui-egui/src/{addins,update,law_tests,app/pdf_import_ui}.rs`,
`apps/wordcraft-cli/src/{law_cli,session_boot,update_check,law_tests}.rs`,
`apps/wordcraft/src/{update_check,update_install}.rs`, plus `crates/law*`, `crates/proof`.

## Fixed by the sync on its own

| Files | Why LawCraft changes them | Fix |
|---|---|---|
| `README.md`, `NOTICE` | LawCraft's own | LawCraft's version |
| `Cargo.lock` | LawCraft's crates | WordCraft's version; `cargo metadata` adds LawCraft's crates back |
| `packaging/windows/package.ps1`, `packaging/windows/wordcraft.wxs` | LawCraft's own MSI: `lawcraft.exe`, its own UpgradeCode, Program Files\LawCraft | LawCraft's version |
| `packaging/macos/package.sh` | `LawCraft.app` and `lawcraft-*` file names | WordCraft's version, renamed by `.github/sync/known_fixes.py` |
| `.github/workflows/windows-arm64.yml` | Install folder `LawCraft`, `lawcraft.exe`, `lawcraft-cli.exe` | WordCraft's version, renamed by `known_fixes.py` |
| `.github/workflows/packaging-lint.yml` | WordCraft's installer-policy job checks WordCraft's MSI only | WordCraft's version, limited to WordCraft's repository by `known_fixes.py` |
| `.github/workflows/release.yml` | LawCraft publishes its own releases (`build-release.yml`, its own permissions and schedule) | LawCraft's version |
| `docs/architecture.md` | The `law`, `law-ui`, `law-docx`, and `proof` rows in the crate table | WordCraft's version; LawCraft rows added by `known_fixes.py` / hand |
| `docs/file-format-parity.md` | LawCraft opens PDFs as editable copies | WordCraft's version with LawCraft's PDF open row, by `known_fixes.py` |

## By hand

### Name, icon and app id (rename only)

`ATTRIBUTION.md`, `ROADMAP.md`, `apps/wordcraft/src/default_apps.rs`,
`apps/wordcraft/src/main.rs` (window title, app id, law install / update thin hooks),
`assets/app-icon/*`, `crates/control-key/src/lib.rs`, `crates/mcp/src/server.rs`
(LawCraft serverInfo + automation trait), `packaging/icons.sh`, `packaging/linux/*`,
`packaging/macos/Info.plist.in`, `packaging/windows/sign.ps1`. Candidates for
`known_fixes.py` the first time one conflicts.

### Workspace and tooling

`Cargo.toml` (law / law-ui / law-docx / proof path deps), `xtask/src/layers.rs` (layer table),
`.gitignore`, `.github/sync/sync.sh`, `.github/sync/known_fixes.py`,
`.github/sync/check_sync_list.py`, `.github/workflows/sync.yml`,
`.github/workflows/build-release.yml`, `.github/workflows/sonarcloud.yml`,
`.github/release/stamp_version.py`. LawCraft deletes WordCraft's `.github/dependabot.yml`.

### Doc model fields LawCraft needs (cannot move)

- `crates/doc/src/para.rs` — `Paragraph::proof_text` for court-check offsets (law proofing).
- `crates/doc/src/section.rs` — `page_num_prefix` on section properties.
- `crates/doc/src/settings.rs` — `doc_vars` storage used by appendix prefix persistence.

### Engine: add-in platform, hooks, I/O (thin hooks; bodies in LawCraft-owned modules)

`crates/engine/Cargo.toml` (law-docx, sha2), `crates/engine/src/lib.rs` (hooks / lint / update
mods, `replaces_document` side-channel), `crates/engine/src/prefs.rs` (`modules`),
`crates/engine/src/session.rs`, `crates/engine/src/session/documents.rs`,
`crates/engine/src/session/snapshot.rs`, `crates/engine/src/session/lifecycle.rs`
(`core_wraps` + `install_commands`; bulk in `session/modules.rs`),
`crates/engine/src/session/layout.rs` (scoped proof-module guard; Pass 3),
`crates/engine/src/ribbon.rs` (add-in tab fields; bulk in `ribbon_addins.rs`),
`crates/engine/src/cmd.rs` (`tracked_edit` / `file_open` / `update` / `core_wraps` mods),
`crates/engine/src/cmd/options.rs` (modules prefs commands),
`crates/engine/src/cmd/file.rs` (one-line PDF open + export-check findings),
`crates/engine/src/cmd/text/autoformat.rs` (`hooks::run_on_text_autoformat` mid-function;
undo timing requires the call site here),
`crates/engine/src/cmd/references/toc.rs` (section page prefix + update-fields hook),
`crates/engine/src/cmd/citations/generated.rs` (no generator in-repo; thin TOA hooks to
`toa_law.rs` — `ENTRY_STYLE` pub + `include_mark` / `format_pages`),
`crates/engine/src/io.rs` (PDF in `OPEN_EXTS`, `OpenedDocument` re-exports; bulk in `io_open.rs`;
one-line `law-docx` page-prefix apply on read).

### PDF import / searchability surface

`crates/pdf/Cargo.toml`, `crates/pdf/src/lib.rs` (`mod import` / `searchability` / extract
re-exports). Implementation lives in LawCraft-owned `import.rs`, `extract.rs`,
`searchability.rs`.

### UI: add-ins, updater, PDF import, rebrand (thin hooks into `addins.rs` / `update.rs`)

`crates/ui-egui/src/lib.rs`, `crates/ui-egui/src/app.rs`,
`crates/ui-egui/src/app/commands.rs` (intercept / decorate_run / update request hooks),
`crates/ui-egui/src/app/files.rs`, `crates/ui-egui/src/app/frame.rs`,
`crates/ui-egui/src/app/prefs.rs`, `crates/ui-egui/src/backstage.rs`,
`crates/ui-egui/src/control/handler.rs` (`invoke_automation`),
`crates/ui-egui/src/file_dialogs.rs`, `crates/ui-egui/src/i18n.rs` (rebrand helpers),

`crates/ui-egui/src/options.rs` (File › Options › Add-ins),
`crates/ui-egui/src/window_ui.rs`.

### Apps / CLI

`apps/wordcraft/Cargo.toml`, `apps/wordcraft-cli/Cargo.toml`,
`apps/wordcraft-cli/src/main.rs` (mods + thin `with_law` / update dispatch; bulk in
`law_cli.rs`, `session_boot.rs`, `update_check.rs`).

### Docs LawCraft adds or extends

`docs/law.md`, `docs/desktop-updates.md`, `docs/lawcraft-sync.md`, `docs/pdf-import.md`,
`docs/gaps.md` (PDF import / law notes).

## In LawCraft's own crates, tied to WordCraft

- `crates/law-ui/Cargo.toml` pins `craft-ui` at the same craft-libs rev as `crates/ui-egui/Cargo.toml`.
- `crates/proof` re-exports craft-proof (same rev as WordCraft) and adds `ModuleEn` plus a
  scoped `enter_session_module` guard (layout / Review proofing only; never a lasting TLS default).
- `crates/law-docx` owns appendix prefix package I/O; engine depends on it for two hooks only.
- `crates/law` registers commands, export checks, TOA/passim helpers, and AutoFormat wraps via
  `install` / `core_wraps`.
- Regression tests live in `crates/law*`, `crates/*/law_tests.rs`, `crates/engine/src/tests_hooks.rs`,
  `apps/wordcraft-cli/tests/law_*.rs`, `crates/ui-egui/tests/law_control_automation.rs`,
  `crates/law/tests/page_num_prefix.rs` covers appendix prefix OOXML (law-docx hooks; no WordCraft file edit).

## Making the list shorter

Each entry that moves out of a WordCraft file into `crates/law*` (or a new LawCraft-owned
module wired by one `mod` / one call) stops conflicting. Prefer hooks and `install_commands` /
`core_wraps` for behaviour that does not need a WordCraft struct field. Struct fields LawCraft
needs (for example `SectionProps::page_num_prefix`) stay where they are.

## Rust toolchain

`rust-toolchain.toml` is LawCraft's own file. It pins Rust 1.99.1 so a newer stable clippy cannot fail LawCraft's CI on WordCraft files LawCraft never edits (Rust 1.100's clippy flagged `crates/formats/src/latex/tests.rs` in WordCraft v0.18.1). When a WordCraft release needs a newer Rust, the sync's gate fails on the toolchain; bump the pin then.
