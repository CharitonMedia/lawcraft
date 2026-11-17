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

Last checked against WordCraft `v0.18.1` (`079f478b`), 2026-11-17. Remaining WordCraft-path edits
are still above the long-term target of ~40 source files; icon assets count as one row. Further
moves into `crates/law*` continue after this sync lands.

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
`apps/wordcraft/src/main.rs` (window title and app id), `assets/app-icon/*`,
`crates/control-key/src/lib.rs`, `crates/mcp/src/server.rs`, `crates/ui-egui/src/chrome.rs`,
`packaging/icons.sh`, `packaging/linux/*`, `packaging/macos/Info.plist.in`,
`packaging/windows/sign.ps1`. Candidates for `known_fixes.py` the first time one conflicts.

### Workspace and tooling

`Cargo.toml` (law / law-ui / law-docx / proof path deps), `xtask/src/layers.rs` (layer table),
`.github/sync/sync.sh`, `.github/sync/known_fixes.py`, `.github/workflows/sync.yml`,
`.github/workflows/build-release.yml`, `.github/workflows/sonarcloud.yml`,
`.github/release/stamp_version.py`. LawCraft deletes WordCraft's `.github/dependabot.yml`.

### The add-in system (LawCraft's own crates plug in here)

`crates/engine/src/lib.rs` (`hooks`, `lint`, `update`, `CommandSpec::replaces_document`),
`crates/engine/src/hooks.rs`, `crates/engine/src/lint.rs`,
`crates/engine/src/session/lifecycle.rs` (`install_commands`),
`crates/engine/src/session.rs` and `session/snapshot.rs` / `session/documents.rs` / `session/layout.rs`
(lint findings, `module_en`, updater fields), `crates/engine/src/prefs.rs` (`modules`),
`crates/engine/src/ribbon.rs` (add-in ribbon tab), `crates/engine/src/cmd.rs`,
`crates/engine/src/cmd/options.rs`, `crates/engine/src/file_access.rs` (confined reads),
`crates/ui-egui/Cargo.toml`, `crates/ui-egui/src/lib.rs`, `crates/ui-egui/src/addins.rs`,
`crates/ui-egui/src/app/commands.rs`, `app/control.rs`, `app/prefs.rs`,
`crates/ui-egui/src/control/handler.rs`, `options.rs` (File › Options › Add-ins),
`widgets/buttons.rs`, `windows.rs`, `window_ui.rs`.

### Hooks the legal add-in uses

- AutoFormat as you type: `crates/engine/src/cmd/text/autoformat.rs` → `hooks::run_on_text_autoformat`.
- Update Fields: `crates/engine/src/cmd/references/fields.rs` → `hooks::run_on_update_fields`.
- Proofing: `crates/layout/src/para/proof.rs`, `crates/layout/src/lib.rs` (`ProofOptions.module`),
  `crates/engine/src/cmd/review/proofing.rs` (court-check findings).
- Print and export checks: `crates/engine/src/print.rs`, `crates/engine/src/cmd/file.rs`.

### Features the legal add-in needs from the engine / doc

- Table of authorities categories and passim: `crates/engine/src/cmd/citations.rs`,
  `citations/generated.rs`, `citations/tests.rs`, `crates/doc/src/para.rs`.
- Appendix page numbers (A1, A2…): `crates/doc/src/section.rs` (`page_num_prefix`),
  `crates/doc/src/settings.rs` (`doc_vars` field), `crates/engine/src/io.rs` /
  `io_ext.rs` (one-line hooks to `wordcraft_law_docx`), `crates/docx/tests/page_num_prefix.rs`
  (regression via law-docx), `crates/engine/src/cmd/insert.rs`, `insert/items.rs`, `cmd/page.rs`,
  `cmd/references/toc.rs`. Prefix persistence itself lives in `crates/law-docx` (not a WordCraft file).
- Styles a court profile defines: `crates/engine/src/cmd/para.rs`, `para/styles.rs` (`apply: false`).
- Caption tables: `crates/engine/src/cmd/tools/specs.rs`, `tools/actions.rs`.
- Whole-document operations: `crates/doc/src/document.rs` (`text_stories`).
- Contact files: `crates/ui-egui/src/file_dialogs.rs` (contacts filter).

### LawCraft's updater (its own releases, not WordCraft's)

`crates/engine/Cargo.toml`, `crates/engine/src/catalog.rs`, `crates/engine/src/update.rs`,
`cmd/update.rs`, `session.rs`, `session/documents.rs`,
`crates/ui-egui/src/backstage.rs`, `update.rs`, `app.rs`, `app/frame.rs`, `app/files.rs`,
`apps/wordcraft/Cargo.toml`, `apps/wordcraft/src/update_check.rs`, `update_install.rs`,
`apps/wordcraft-cli/Cargo.toml`, `apps/wordcraft-cli/src/main.rs`, `update_check.rs`,
`apps/wordcraft-cli/tests/it/help.rs`, `tests/update_stdout.rs`.

### PDF import as an editable copy

`crates/pdf/Cargo.toml`, `crates/pdf/src/lib.rs`, `extract.rs`, `import.rs`, `import/tests.rs`,
`crates/engine/src/io.rs`, `cmd/file.rs`, `crates/ui-egui/src/dialogs.rs`, `dialogs/chrome.rs`,
`dialogs/dispatch.rs`, `app/files.rs`, `app/pictures.rs`, `i18n.rs`,
`apps/wordcraft-cli/src/main.rs`, `docs/gaps.md`, `docs/pdf-import.md`.

### Docs and LawCraft-specific CLI tests (still under WordCraft paths)

`docs/law.md`, `docs/desktop-updates.md`, `docs/lawcraft-sync.md`,
`apps/wordcraft-cli/tests/law_convert.rs`, `law_mcp_verification.rs`,
`crates/ui-egui/tests/law_control_automation.rs`, `crates/mcp/src/lib.rs`.

### Tests that still live in WordCraft test files

`crates/engine/src/tests.rs`, `tests/fonts.rs`, `tests_hooks.rs`,
`cmd/references/tests.rs`, `cmd/tools/tests.rs`, `cmd/citations/tests.rs`,
`crates/doc/src/tests.rs`, `crates/layout/src/tests.rs`, `crates/pdf/src/tests.rs`,
`crates/ui-egui/src/tests.rs`, `backstage/tests.rs`, `canvas/tests.rs`,
`file_dialogs/tests.rs`, `control/doc_tests.rs`, `apps/wordcraft-cli/src/tests.rs`.
Self-contained LawCraft cases continue to move into `crates/law*` / `crates/law-ui` tests.

## In LawCraft's own crates, tied to WordCraft

- `crates/law-ui/Cargo.toml` pins `craft-ui` at the same craft-libs rev as `crates/ui-egui/Cargo.toml`.
- `crates/proof` re-exports craft-proof (same rev as WordCraft) and adds `ModuleEn`.
- `crates/law-docx` owns appendix prefix package I/O; engine depends on it for two hooks only.

## Making the list shorter

Each entry that moves out of a WordCraft file into `crates/law*` stops conflicting. Appendix
prefix persistence already moved to `law-docx`. Prefer hooks (`hooks.rs`) for behaviour that can
run without a WordCraft struct field. Struct fields LawCraft needs (for example
`SectionProps::page_num_prefix`) stay where they are.
