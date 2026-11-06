#!/usr/bin/env bash
# Extracts the game's built-in English global.ini from Data.p4k with the StarBreaker CLI.
#
# The game keeps its English texts inside Data.p4k; a language pack on disk replaces them. Readers need these
# texts (ADR 0007), so players without a pack need this file. It is written as global-english-<version>.ini into
# the output folder, the version from build_manifest.id next to Data.p4k, so a stale file is obvious after a patch.
#
# Needs the StarBreaker CLI (https://github.com/w00zla/starbreaker-qm, `starbreaker p4k extract`).
#
# Usage: scripts/extract-global-ini.sh <starbreaker> [Data.p4k (default $SC_DATA_P4K)] [out dir (default .)]
set -euo pipefail

starbreaker=${1:?usage: extract-global-ini.sh <starbreaker> [Data.p4k] [out dir]}
p4k=${2:-${SC_DATA_P4K:-}}
out=${3:-.}
[ -n "$p4k" ] || { echo "Data.p4k: pass it or set SC_DATA_P4K" >&2; exit 1; }
[ -f "$p4k" ] || { echo "Data.p4k not found: $p4k" >&2; exit 1; }

version=unknown
manifest="$(dirname "$p4k")/build_manifest.id"
if [ -f "$manifest" ]; then
  version=$(grep -o '"Version": *"[^"]*"' "$manifest" | head -1 | sed 's/.*"\([^"]*\)"$/\1/')
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$starbreaker" p4k extract --p4k "$p4k" --filter 'Data/Localization/english/global.ini' -o "$tmp"
file=$(find "$tmp" -name global.ini -print -quit)
[ -n "$file" ] || { echo "global.ini not found in Data.p4k" >&2; exit 1; }
mkdir -p "$out"
dest="$out/global-english-$version.ini"
cp "$file" "$dest"
echo "$dest"
