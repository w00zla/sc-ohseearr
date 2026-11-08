#!/usr/bin/env bash
# Extracts the game's UI fonts from Data.p4k with the StarBreaker CLI and converts them to TrueType for training.
#
# Star Citizen keeps its UI fonts in the Scaleform font library Data/UI/fonts/Shared/fonts_en.swf inside Data.p4k
# (29 fonts, among them Blender Pro, a commercial font). The recognizer recipes draw synthetic lines in them
# (synth.fonts.dir: fonts-sc/ttf in the data root). This extracts the SWF and runs training/tools/swf2ttf.py over
# it: one <Family>-<Style>.ttf per font in the output folder. The fonts are for local training only: never put them
# into git or a build.
#
# Needs a StarBreaker CLI with `p4k extract`: upstream (https://github.com/diogotr7/StarBreaker) or a fork such as
# starbreaker-ps (https://github.com/w00zla/starbreaker-ps), and the training venv's Python with fontTools
# (training/README.md).
#
# Usage: scripts/extract-fonts.sh <starbreaker> [Data.p4k (default $SC_DATA_P4K)]
#        [out dir (default <data root>/fonts-sc/ttf)] [python (default local/detector/.venv/bin/python)]
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
starbreaker=${1:?usage: extract-fonts.sh <starbreaker> [Data.p4k] [out dir] [python]}
p4k=${2:-${SC_DATA_P4K:-}}
out=${3:-${SC_OHSEEARR_DATA:-$repo/local}/fonts-sc/ttf}
python=${4:-$repo/local/detector/.venv/bin/python}
[ -n "$p4k" ] || { echo "Data.p4k: pass it or set SC_DATA_P4K" >&2; exit 1; }
[ -f "$p4k" ] || { echo "Data.p4k not found: $p4k" >&2; exit 1; }
[ -x "$python" ] || { echo "Python not found: $python (pass it, see training/README.md)" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
"$starbreaker" p4k extract --p4k "$p4k" --filter 'Data/UI/fonts/Shared/fonts_en.swf' -o "$tmp"
swf=$(find "$tmp" -name fonts_en.swf -print -quit)
[ -n "$swf" ] || { echo "fonts_en.swf not found in Data.p4k" >&2; exit 1; }
"$python" -I "$repo/training/tools/swf2ttf.py" "$swf" "$out"
