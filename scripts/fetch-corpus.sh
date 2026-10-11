#!/usr/bin/env bash
# Fetch the regression corpus listed in corpus/manifest.toml into corpus/cache/<name>/.
#
# Each project is fetched at its pinned commit with a shallow, blob-filtered
# fetch and a sparse checkout that only materialises KiCad source files, so a
# large hardware repo costs megabytes rather than gigabytes. Projects already
# checked out at the pinned commit are skipped.
#
# Usage: scripts/fetch-corpus.sh [name ...]   (no names = every entry)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$root/corpus/manifest.toml"
cache="$root/corpus/cache"

command -v python3 >/dev/null || { echo "fetch-corpus: python3 (3.11+, for tomllib) is required" >&2; exit 1; }
command -v git >/dev/null || { echo "fetch-corpus: git is required" >&2; exit 1; }

# Only KiCad design sources (plus library tables) are checked out.
sparse_patterns=(
  '*.kicad_pro' '*.kicad_sch' '*.kicad_pcb' '*.kicad_dru' '*.kicad_wks'
  '*.kicad_sym' '*.kicad_mod' 'fp-lib-table' 'sym-lib-table'
)

# Emit one tab-separated line per entry: name, repo, commit, project paths...
entries="$(python3 -I - "$manifest" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    data = tomllib.load(f)
for p in data["project"]:
    print("\t".join([p["name"], p["repo"], p["commit"], *p["projects"]]))
PY
)"

want=("$@")
selected() {
  [[ ${#want[@]} -eq 0 ]] && return 0
  local w
  for w in "${want[@]}"; do [[ "$w" == "$1" ]] && return 0; done
  return 1
}

fetched=0 skipped=0 failed=()
mkdir -p "$cache"

while IFS=$'\t' read -r -a fields; do
  name="${fields[0]}" repo="${fields[1]}" commit="${fields[2]}"
  projects=("${fields[@]:3}")
  selected "$name" || continue
  dest="$cache/$name"

  if [[ -d "$dest/.git" ]] && [[ "$(git -C "$dest" rev-parse -q --verify HEAD 2>/dev/null || true)" == "$commit" ]]; then
    echo "skip   $name (already at ${commit:0:12})"
    skipped=$((skipped + 1))
  else
    echo "fetch  $name  $repo @ ${commit:0:12}"
    rm -rf "$dest"
    mkdir -p "$dest"
    if ! {
      git -C "$dest" init -q &&
        git -C "$dest" remote add origin "$repo" &&
        git -C "$dest" config advice.detachedHead false &&
        git -C "$dest" sparse-checkout set --no-cone "${sparse_patterns[@]}" &&
        git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$commit" &&
        git -C "$dest" checkout -q FETCH_HEAD
    }; then
      echo "FAIL   $name: fetch failed" >&2
      failed+=("$name")
      rm -rf "$dest"
      continue
    fi
    fetched=$((fetched + 1))
  fi

  # Every listed .kicad_pro must exist, with its schematic and board beside it.
  for pro in "${projects[@]}"; do
    base="${pro%.kicad_pro}"
    for f in "$pro" "$base.kicad_sch" "$base.kicad_pcb"; do
      if [[ ! -f "$dest/$f" ]]; then
        echo "FAIL   $name: missing $f" >&2
        failed+=("$name")
      fi
    done
  done
done <<<"$entries"

echo "fetched $fetched, skipped $skipped, failed ${#failed[@]}"
if [[ ${#failed[@]} -gt 0 ]]; then
  printf 'failed: %s\n' "${failed[@]}" | sort -u >&2
  exit 1
fi
