"""Resolve a merge conflict in an interface catalog (.tsv) by keeping both sides' rows.

WordCraft and LawCraft both add rows at the end of each catalog, so their additions meet in a
conflict; every row from either side is kept once, WordCraft's first.
"""
import re
import sys

CONFLICT = re.compile(r"<<<<<<< [^\n]*\n(.*?)\n=======\n(.*?)\n>>>>>>> [^\n]*", re.S)


def keep_both(m):
    seen, out = set(), []
    for line in m.group(2).split("\n") + m.group(1).split("\n"):
        if line and line in seen:
            continue
        seen.add(line)
        out.append(line)
    return "\n".join(out)


for path in sys.argv[1:]:
    with open(path, encoding="utf-8") as f:
        text = f.read()
    merged = CONFLICT.sub(keep_both, text)
    if "<<<<<<<" in merged:
        sys.exit(f"{path}: a conflict that isn't rows at the end")
    with open(path, "w", encoding="utf-8") as f:
        f.write(merged)
