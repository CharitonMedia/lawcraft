#!/usr/bin/env python3
"""Known fixes for the WordCraft merge (docs/lawcraft-sync.md lists them).

Called by sync.sh with the conflicted path. Takes WordCraft's version of the file and makes
LawCraft's known changes to it, so packaging and workflow files LawCraft only renames never need
a person. Prints "fixed" and exits 0 when it handled the file; exits 1 when the file isn't one it
knows, or when WordCraft's version no longer has the text a fix expects (then a person looks).
"""

import subprocess
import sys


def theirs(path):
    subprocess.run(["git", "checkout", "--theirs", "--", path], check=True)
    with open(path, encoding="utf-8") as f:
        return f.read()


def replace_all(text, pairs, path):
    for old, new in pairs:
        if old not in text:
            sys.exit(f"known fix for {path}: WordCraft's version no longer has {old!r}")
        text = text.replace(old, new)
    return text


def macos_package(path):
    """LawCraft.app and lawcraft-*.dmg/zip names; the binary inside is still built as wordcraft."""
    text = theirs(path)
    return replace_all(
        text,
        [
            ("$DIST/wordcraft-<version>-macos-<arch>.dmg          WordCraft.app", "$DIST/lawcraft-<version>-macos-<arch>.dmg          LawCraft.app"),
            ("$DIST/wordcraft-cli-<version>-macos-<arch>.zip", "$DIST/lawcraft-cli-<version>-macos-<arch>.zip"),
            ('DMG="$DIST/wordcraft-$VERSION', 'DMG="$DIST/lawcraft-$VERSION'),
            ('CLI_ZIP="$DIST/wordcraft-cli-', 'CLI_ZIP="$DIST/lawcraft-cli-'),
            ('echo "==> WordCraft $VERSION', 'echo "==> LawCraft $VERSION'),
            ('CLI_DIR="$WORK/wordcraft-cli-', 'CLI_DIR="$WORK/lawcraft-cli-'),
            ("WordCraft.app", "LawCraft.app"),
            ('"$APP/Contents/MacOS/WordCraft"', '"$APP/Contents/MacOS/LawCraft"'),
            ("Resources/WordCraft.icns", "Resources/LawCraft.icns"),
            ("WordCraft-notarize.zip", "LawCraft-notarize.zip"),
            ('-volname "WordCraft"', '-volname "LawCraft"'),
        ],
        path,
    )


def windows_arm64(path):
    """LawCraft's install folder and binary names; LawCraft keeps its own MSI, so no new install contract."""
    lines = []
    for line in theirs(path).splitlines(keepends=True):
        key = line.strip().split(":")[0]
        if key in ("install-relative-path", "install-scope", "main-binary", "cli-binary"):
            continue
        if key == "app-dir":
            indent = line[: len(line) - len(line.lstrip())]
            lines.append(f"{indent}app-dir: LawCraft\n{indent}main-binary: lawcraft.exe\n{indent}cli-binary: lawcraft-cli.exe\n")
            continue
        lines.append(line)
    text = "".join(lines)
    if "app-dir: LawCraft" not in text:
        sys.exit(f"known fix for {path}: no app-dir in WordCraft's version")
    return text


def packaging_lint(path):
    """WordCraft's own installer policy job runs only in WordCraft's repository."""
    text = theirs(path)
    marker = "    if: github.repository == 'storytold/wordcraft'\n"
    if marker in text or "  windows-install-policy:\n" not in text:
        return text
    return text.replace(
        "  windows-install-policy:\n",
        "  windows-install-policy:\n    # WordCraft's own installer policy; LawCraft packages with its own MSI (packaging/windows).\n" + marker,
        1,
    )


FIXES = {
    "packaging/macos/package.sh": macos_package,
    ".github/workflows/windows-arm64.yml": windows_arm64,
    ".github/workflows/packaging-lint.yml": packaging_lint,
}


def main():
    path = sys.argv[1]
    fix = FIXES.get(path)
    if fix is None:
        sys.exit(1)
    text = fix(path)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    subprocess.run(["git", "add", "--", path], check=True)
    print("fixed")


if __name__ == "__main__":
    main()
