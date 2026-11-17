#!/usr/bin/env python3
"""Check that every path on stdin appears in docs/lawcraft-sync.md.

Reads conflicted (or candidate) paths from stdin, one per line. Parses the sync
doc for backtick-wrapped paths and simple globs (`assets/app-icon/*`,
`packaging/linux/*`). A path matches when:

- it equals a listed path,
- it ends with `/` + a listed relative path (e.g. `app/control.rs`),
- it matches a listed glob (`fnmatch`), or
- it sits under a listed `dir/*` prefix.

Exits 0 and prints nothing when every path is listed. Exits 1 and prints the
unlisted paths (space-separated) on stdout when any are missing.
"""

from __future__ import annotations

import fnmatch
import re
import sys
from pathlib import Path


def entries(doc: str) -> list[str]:
    return re.findall(r"`([^`]+)`", doc)


def listed(path: str, items: list[str]) -> bool:
    for e in items:
        if "*" in e:
            if fnmatch.fnmatch(path, e) or fnmatch.fnmatch(path, "*/" + e):
                return True
            if e.endswith("/*"):
                prefix = e[:-1]  # keep trailing slash intent: "dir/"
                base = e[:-2]
                if path == base or path.startswith(base + "/") or path.startswith(prefix):
                    return True
            continue
        if path == e or path.endswith("/" + e):
            return True
    return False


def main() -> int:
    doc_path = Path("docs/lawcraft-sync.md")
    if not doc_path.is_file():
        print("error: docs/lawcraft-sync.md missing", file=sys.stderr)
        return 2
    items = entries(doc_path.read_text(encoding="utf-8"))
    paths = [ln.strip() for ln in sys.stdin if ln.strip()]
    missing = [p for p in paths if not listed(p, items)]
    if missing:
        sys.stdout.write(" ".join(missing) + ("\n" if missing else ""))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
