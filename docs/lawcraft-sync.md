# LawCraft and WordCraft: what LawCraft changes in WordCraft's files

LawCraft is WordCraft plus the legal add-in (`crates/law`, `crates/law-ui`). Every night
`.github/sync/sync.sh` merges WordCraft's `main`. LawCraft's own crates never conflict; the
conflicts come from the lines LawCraft changes inside WordCraft's files. This page lists every
such file, why LawCraft changes it, and what happens when a merge conflicts there.

**Fixed by the sync on its own** means `sync.sh` resolves the conflict and the nightly gate still
has to pass before anything is pushed. **By hand** means the sync stops, opens the "Nightly
WordCraft sync needs attention" issue and assigns it, so GitHub emails the maintainer; the fix is
to take WordCraft's version and add LawCraft's lines back where WordCraft now keeps that code.

Last checked against WordCraft 0.12.0 (`3de6ff95`), 2026-10-28.

## Fixed by the sync on its own

| Files | Why LawCraft changes them | Fix |
|---|---|---|
| `README.md`, `NOTICE` | LawCraft's own | LawCraft's version |
| `Cargo.lock` | LawCraft's crates | WordCraft's version; `cargo metadata` adds LawCraft's crates back |
| `packaging/windows/package.ps1`, `packaging/windows/wordcraft.wxs` | LawCraft's own MSI: `lawcraft.exe`, its own UpgradeCode, Program Files\LawCraft | LawCraft's version. WordCraft moved to one per-user MSI under `ArtCraft\WordCraft` with an install-policy check; LawCraft has not taken that on |
| `packaging/macos/package.sh` | `LawCraft.app` and `lawcraft-*` file names | WordCraft's version, renamed by `.github/sync/known_fixes.py` |
| `.github/workflows/windows-arm64.yml` | Install folder `LawCraft`, `lawcraft.exe`, `lawcraft-cli.exe` | WordCraft's version, renamed by `known_fixes.py` |
| `.github/workflows/packaging-lint.yml` | WordCraft's installer-policy job checks WordCraft's MSI only | WordCraft's version, the job limited to WordCraft's repository by `known_fixes.py` |

## By hand

### Name, icon and app id (rename only)

`.github/workflows/release.yml`, `ATTRIBUTION.md`, `ROADMAP.md`, `apps/wordcraft/src/default_apps.rs`,
`apps/wordcraft/src/main.rs` (window title and app id), `assets/app-icon/*`, `crates/control-key/src/lib.rs`,
`crates/mcp/src/server.rs`, `crates/ui-egui/src/chrome.rs`, `packaging/icons.sh`, `packaging/linux/*`,
`packaging/macos/Info.plist.in`, `packaging/windows/sign.ps1`. These are candidates for `known_fixes.py`
the first time one conflicts.

### The add-in system (LawCraft's own crates plug in here)

`Cargo.toml` (the `law` crates), `crates/engine/src/lib.rs` (`hooks`, `lint`,
`CommandSpec::replaces_document`), `crates/engine/src/session/lifecycle.rs` (`install_commands`),
`crates/engine/src/session.rs` and `session/snapshot.rs` (lint findings), `crates/engine/src/prefs.rs`
(`modules`), `crates/engine/src/ribbon.rs` (the add-in's ribbon tab), `crates/engine/src/cmd.rs`,
`crates/engine/src/cmd/options.rs`, `crates/engine/src/file_access.rs` (confined reads),
`crates/ui-egui/Cargo.toml`, `crates/ui-egui/src/lib.rs`, `app/commands.rs` (`file.options
{"modules"}` turns an add-in on or off), `app/control.rs`, `app/prefs.rs`, `control/handler.rs`,
`options.rs` (File › Options › Add-ins), `widgets/buttons.rs`, `windows.rs`, `window_ui.rs`,
`xtask/src/layers.rs`, `docs/architecture.md`.

### Hooks the legal add-in uses

- AutoFormat as you type: `crates/engine/src/cmd/text/autoformat.rs` calls `hooks::run_on_text_autoformat`.
- Update Fields: `crates/engine/src/cmd/references/fields.rs` calls `hooks::run_on_update_fields`.
- Proofing: `crates/layout/src/para/proof.rs`, `crates/layout/src/lib.rs` (`ProofOptions`),
  `crates/proof/src/lib.rs` (the legal word list), `crates/engine/src/cmd/review/proofing.rs`
  (court-check findings in Review › Proofing).
- Print and export checks: `crates/engine/src/print.rs`, `crates/engine/src/cmd/file.rs`.

### Features the legal add-in needs from the engine

- Table of authorities categories and passim: `crates/engine/src/cmd/citations.rs`,
  `citations/generated.rs`, `citations/tests.rs`, `crates/doc/src/para.rs`.
- Appendix page numbers (A1, A2…): `crates/doc/src/section.rs`, `crates/doc/src/settings.rs`
  (`doc_vars`), `crates/docx/src/lib.rs`, `read.rs`, `read/meta.rs`, `read/props.rs`,
  `write/settings.rs`, `write/settings_merge.rs`, `crates/engine/src/cmd/insert.rs`,
  `insert/items.rs`, `cmd/page.rs`, `cmd/references/toc.rs`.
- Styles a court profile defines: `crates/engine/src/cmd/para.rs`, `para/styles.rs` (`apply: false`, `basedOn`).
- Caption tables: `crates/engine/src/cmd/tools/specs.rs`, `tools/actions.rs` (`table.cellMargins`).
- Whole-document operations: `crates/doc/src/document.rs` (`text_stories`).
- Contact files: `crates/ui-egui/src/file_dialogs.rs` (the `contacts` filter).

### LawCraft's updater (its own releases, not WordCraft's)

`crates/engine/Cargo.toml`, `crates/engine/src/catalog.rs`, `session.rs`, `session/documents.rs`,
`crates/ui-egui/src/backstage.rs`, `updates_ui.rs`, `app.rs`, `app/frame.rs`, `app/files.rs`,
`apps/wordcraft/Cargo.toml`, `apps/wordcraft-cli/Cargo.toml`, `apps/wordcraft-cli/src/main.rs`,
`apps/wordcraft-cli/tests/it/help.rs`.

### PDF import as an editable copy

`crates/pdf/Cargo.toml`, `crates/pdf/src/lib.rs`, `crates/engine/src/io.rs`, `cmd/file.rs`,
`crates/ui-egui/src/dialogs.rs`, `dialogs/chrome.rs`, `dialogs/dispatch.rs`, `app/files.rs`,
`app/pictures.rs`, `i18n.rs`, `apps/wordcraft-cli/src/main.rs`, `docs/file-format-parity.md`, `docs/gaps.md`.

### Tests that go with the above

`crates/engine/src/tests.rs`, `tests/fonts.rs`, `cmd/references/tests.rs`, `cmd/tools/tests.rs`,
`crates/doc/src/tests.rs`, `crates/layout/src/tests.rs`, `crates/pdf/src/tests.rs`,
`crates/ui-egui/src/tests.rs`, `backstage/tests.rs`, `canvas/tests.rs`, `file_dialogs/tests.rs`,
`control/doc_tests.rs`, `apps/wordcraft-cli/src/tests.rs`.

## In LawCraft's own crates, tied to WordCraft

- `crates/law-ui/Cargo.toml` pins `craft-ui` at the same craft-libs rev as
  `crates/ui-egui/Cargo.toml`. When WordCraft moves that rev, move this one with it.

## Making the list shorter

Each entry that moves out of a WordCraft file into `crates/law*` stops conflicting. The hooks
above exist for that: a feature that can run through a hook belongs in LawCraft's crates.
