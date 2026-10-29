"""Stamp the release version into the build checkout before compiling.

The running app reports its version from Cargo.toml, and LawCraft's updater compares that to the
newest release. Without this the installed app would keep WordCraft's version and offer the same
update forever. Runs in the release build's throwaway checkout only; nothing is committed.
"""
import os
import re
import sys

new = os.environ.get("VERSION", "").strip()
if not new:
    sys.exit("error: VERSION is not set")

with open("Cargo.toml", encoding="utf-8") as f:
    toml = f.read()
m = re.search(r'(\[workspace\.package\][^\[]*?\nversion\s*=\s*")([^"]+)(")', toml)
if not m:
    sys.exit("error: no [workspace.package] version in Cargo.toml")
old = m.group(2)
if old == new:
    print(f"version already {new}")
    sys.exit(0)
toml = toml[: m.start(2)] + new + toml[m.end(2) :]
with open("Cargo.toml", "w", encoding="utf-8", newline="") as f:
    f.write(toml)

# Workspace crates are the Cargo.lock entries with no source line.
with open("Cargo.lock", encoding="utf-8") as f:
    blocks = f.read().split("\n[[package]]\n")
count = 0
for i, b in enumerate(blocks[1:], 1):
    if "\nsource = " not in b and f'\nversion = "{old}"' in b:
        blocks[i] = b.replace(f'\nversion = "{old}"', f'\nversion = "{new}"', 1)
        count += 1
with open("Cargo.lock", "w", encoding="utf-8", newline="") as f:
    f.write("\n[[package]]\n".join(blocks))
print(f"stamped {old} -> {new} (Cargo.toml and {count} workspace crates in Cargo.lock)")
