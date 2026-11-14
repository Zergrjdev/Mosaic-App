#!/bin/bash
# Copies a freshly generated downloads.json into a checked-out website repository and pushes it, so the
# website's download page links the new release without anyone copying files by hand.
#   publish-downloads.sh <downloads.json> <repo-dir> <path-inside-repo> <commit message>
# Run it with bash (the executable flag can be lost when committing from Windows).
# Does nothing (and succeeds) when the file did not change. Retries when someone else pushed first.
set -euo pipefail
JSON="$(realpath "$1")"; REPO="$2"; DEST="$3"; MSG="$4"
cd "$REPO"
mkdir -p "$(dirname "$DEST")"
cp "$JSON" "$DEST"
if [ -z "$(git status --porcelain -- "$DEST")" ]; then echo "downloads.json is already up to date"; exit 0; fi
git config user.name  "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add -- "$DEST"
git commit -q -m "$MSG"
for attempt in 1 2 3; do
  if git push -q; then echo "pushed $DEST"; exit 0; fi
  echo "push rejected (attempt $attempt), rebasing"
  git pull -q --rebase
done
echo "could not push $DEST" >&2; exit 1
