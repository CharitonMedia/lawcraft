#!/usr/bin/env bash
# Merge WordCraft's newest release tag into LawCraft's main (run by .github/workflows/sync.yml).
#
# Resolves the conflicts LawCraft expects on every merge (docs/lawcraft-sync.md lists them):
# README.md, NOTICE and release.yml (LawCraft's), Cargo.lock (WordCraft's, then cargo adds LawCraft's crates
# back), the Windows installer (LawCraft's), and the packaging and workflow files LawCraft only
# renames, plus the two docs LawCraft adds rows to (WordCraft's, fixed by known_fixes.py). Any other conflict stops the merge and reports
# the files on $GITHUB_OUTPUT.
set -euo pipefail
out="${GITHUB_OUTPUT:-/dev/stdout}"

git remote get-url upstream >/dev/null 2>&1 || git remote add upstream https://github.com/storytold/wordcraft.git
git fetch --quiet upstream --tags

# Prefer pure vMAJOR.MINOR.PATCH (stable finals). Fall back to prerelease tags only if none exist.
pick_newest_release_tag() {
  local tags pure
  tags="$(git ls-remote --tags --refs upstream 'v*' | awk '{print $2}' | sed 's|refs/tags/||')"
  if [ -z "$tags" ]; then
    echo "error: no v* tags on upstream" >&2
    return 1
  fi
  pure="$(printf '%s\n' "$tags" | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' || true)"
  if [ -n "$pure" ]; then
    printf '%s\n' "$pure" | sort -V | tail -n 1
  else
    printf '%s\n' "$tags" | sort -V | tail -n 1
  fi
}

tag="$(pick_newest_release_tag)"
git fetch --quiet upstream "refs/tags/${tag}:refs/tags/${tag}"

if git merge-base --is-ancestor "$tag" HEAD; then
  echo "LawCraft already has WordCraft $tag ($(git rev-parse --short "$tag"))"
  echo "result=current" >>"$out"
  exit 0
fi

# Every conflict path must appear in docs/lawcraft-sync.md (backticks or globs).
msg="Merge WordCraft $tag ($(git rev-parse --short "$tag"))"
if ! git merge --no-edit -m "$msg" "$tag"; then
  unlisted="$(git diff --name-only --diff-filter=U | .github/sync/check_sync_list.py || true)"
  if [ -n "$unlisted" ]; then
    echo "conflicts=$unlisted" >>"$out"
    git merge --abort
    echo "error: conflicting files not listed in docs/lawcraft-sync.md: $unlisted" >&2
    exit 1
  fi
  while IFS= read -r f; do
    case "$f" in
      README.md | NOTICE) git checkout --ours -- "$f" && git add -- "$f" ;;
      Cargo.lock) git checkout --theirs -- "$f" && git add -- "$f" ;;
      # LawCraft builds its own MSI (lawcraft.exe, its own UpgradeCode); WordCraft's changes to
      # its installer don't apply.
      packaging/windows/package.ps1 | packaging/windows/wordcraft.wxs) git checkout --ours -- "$f" && git add -- "$f" ;;
      # LawCraft publishes its own releases (build-release.yml, its own permissions and schedule).
      .github/workflows/release.yml) git checkout --ours -- "$f" && git add -- "$f" ;;
      # Files LawCraft only renames: WordCraft's version with LawCraft's names (known_fixes.py).
      *) .github/sync/known_fixes.py "$f" >/dev/null || true ;;
    esac
  done < <(git diff --name-only --diff-filter=U)
  left="$(git diff --name-only --diff-filter=U | tr '\n' ' ')"
  if [ -n "$left" ]; then
    echo "conflicts=$left" >>"$out"
    git merge --abort
    echo "error: conflicts that need a person: $left" >&2
    exit 1
  fi
  git commit --no-edit --quiet
fi

# WordCraft's Cargo.lock lacks LawCraft's own crates; resolving the workspace adds them back.
cargo metadata --format-version 1 >/dev/null
if ! git diff --quiet -- Cargo.lock; then
  git commit --quiet -m "Cargo.lock after merging WordCraft" -- Cargo.lock
fi
echo "result=merged" >>"$out"
echo "merged WordCraft $tag ($(git rev-parse --short "$tag"))"
