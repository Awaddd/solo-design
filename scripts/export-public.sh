#!/usr/bin/env bash
# Usage: scripts/export-public.sh <dest>
# Exports the files listed in scripts/public-manifest.txt, read from HEAD,
# into a fresh git repository at <dest> with one commit and no remote.
# Uncommitted edits and untracked files never reach the export.
set -euo pipefail

die() { echo "export-public: $*" >&2; exit 1; }

[ $# -eq 1 ] || die "usage: scripts/export-public.sh <dest>"
dest=$1
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
manifest=scripts/public-manifest.txt

if [ -e "$dest" ] && [ -n "$(ls -A "$dest" 2>/dev/null)" ]; then
  die "destination $dest exists and is not empty"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Tracked files at HEAD (the manifest itself is read from HEAD too).
git ls-tree -r --name-only HEAD > "$tmp/tracked"
git show "HEAD:$manifest" > "$tmp/manifest" || die "$manifest is not committed"

includes=(); excludes=()
while IFS= read -r line || [ -n "$line" ]; do
  line=${line%%#*}
  line=$(printf '%s' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
  [ -n "$line" ] || continue
  if [ "${line:0:1}" = "!" ]; then
    x=$(printf '%s' "${line:1}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    excludes+=("$x")
  else
    includes+=("$line")
  fi
done < "$tmp/manifest"
[ ${#includes[@]} -gt 0 ] || die "$manifest lists no paths"

covers() { # entry file
  local e=${1%/}
  [ "$2" = "$e" ] || [[ "$2" == "$e/"* ]]
}

overlay=scripts/public-overlay
: > "$tmp/files"
for entry in "${includes[@]}"; do
  found=0
  while IFS= read -r f; do
    if covers "$entry" "$f"; then
      found=1
      echo "$f" >> "$tmp/files"
    fi
  done < "$tmp/tracked"
  [ "$found" -eq 1 ] || die "manifest path not tracked at HEAD: $entry"
done

sort -u "$tmp/files" > "$tmp/files.sorted"
: > "$tmp/final"
while IFS= read -r f; do
  skip=0
  # The overlay itself is never exported (its files are applied below).
  covers "$overlay" "$f" && skip=1
  for x in "${excludes[@]+"${excludes[@]}"}"; do
    if covers "$x" "$f"; then skip=1; break; fi
  done
  [ "$skip" -eq 1 ] || echo "$f" >> "$tmp/final"
done < "$tmp/files.sorted"
[ -s "$tmp/final" ] || die "the manifest selects no files"

# Overlay: scripts/public-overlay/<path> replaces <path> in the export. The
# target must be exported by the manifest, or the export fails naming it.
: > "$tmp/overlay"
while IFS= read -r f; do
  covers "$overlay" "$f" || continue
  rel=${f#"$overlay"/}
  grep -qxF -- "$rel" "$tmp/final" || die "overlay path is not exported by the manifest: $rel"
  echo "$rel" >> "$tmp/overlay"
done < "$tmp/tracked"

mkdir -p "$dest"
git archive HEAD | tar -x -C "$dest" --files-from="$tmp/final"
while IFS= read -r rel; do
  git show "HEAD:$overlay/$rel" > "$dest/$rel"
done < "$tmp/overlay"
git -C "$dest" init -q
git -C "$dest" add -A
git -C "$dest" commit -q -m "Initial public export"
echo "export-public: $(wc -l < "$tmp/final") files exported to $dest"
