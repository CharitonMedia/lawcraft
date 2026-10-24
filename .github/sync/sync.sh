#!/usr/bin/env bash
# Merge WordCraft's main into LawCraft's main (run by .github/workflows/sync.yml).
#
# Resolves the conflicts LawCraft expects on every merge: LawCraft's own interface catalogs (LawCraft's),
# README.md and NOTICE (LawCraft's), Cargo.lock (WordCraft's, then cargo adds LawCraft's crates
# back). Any other conflict stops the merge and reports the files on $GITHUB_OUTPUT.
set -euo pipefail
out="${GITHUB_OUTPUT:-/dev/stdout}"

git remote get-url upstream >/dev/null 2>&1 || git remote add upstream https://github.com/storytold/wordcraft.git
git fetch --quiet upstream main
if git merge-base --is-ancestor upstream/main HEAD; then
  echo "LawCraft already has WordCraft $(git rev-parse --short upstream/main)"
  echo "result=current" >>"$out"
  exit 0
fi

msg="Merge WordCraft main ($(git rev-parse --short upstream/main))"
if ! git merge --no-edit -m "$msg" upstream/main; then
  while IFS= read -r f; do
    case "$f" in
      crates/ui-egui/lawcraft-i18n/*.tsv) git checkout --ours -- "$f" && git add -- "$f" ;;
      README.md | NOTICE) git checkout --ours -- "$f" && git add -- "$f" ;;
      Cargo.lock) git checkout --theirs -- "$f" && git add -- "$f" ;;
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
echo "merged WordCraft $(git rev-parse --short upstream/main)"
