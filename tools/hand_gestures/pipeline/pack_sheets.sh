#!/usr/bin/env bash
# usage: pack_sheets.sh <dir holding hand-swipe/ and hand-hold/> <work dir> [dest dir]
#
# Each run stages in a fresh pack.XXXXXX/ under the work dir, so tiles from an earlier run can't end
# up in a sheet and nothing already there is touched.
#
# The four packing passes, in the order they were run. Each pass reads the previous pass's
# sheets, so the lossy WebP steps compound exactly as they did for the checked-in sheets.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
FRAMES=$1
WORK=$(mkdir -p "$2" && cd "$2" && pwd)
WORK=$(mktemp -d "$WORK/pack.XXXXXX")
DEST=${3:-$HERE/../source}
# Without a font montage reports an error and exits non-zero even though the sheet it writes is
# fine; naming one keeps strict mode usable and changes no pixels.
FONT=${MONTAGE_FONT:-/System/Library/Fonts/Supplemental/Arial.ttf}

mkdir -p "$WORK/1"
python3 "$HERE/first_pack.py" "$FRAMES" "$WORK/1"

mkdir -p "$WORK/2/holdtiles"
cp "$WORK/1/hand-swipe-sheet.webp" "$WORK/2/hand-swipe-sheet.webp"
magick "$WORK/1/hand-hold-sheet.webp" -crop 248x405 +repage "$WORK/2/holdtiles/t%03d.png"
magick montage -font "$FONT" "$WORK"/2/holdtiles/t*.png -tile 15x -geometry +0+0 -background none -define webp:lossless=false -quality 90 "$WORK/2/hand-hold-sheet.webp"

# An 8 px transparent margin round every cell, because filtering bled one frame's wrist into
# the next frame's edge.
S="$WORK/3"; H="$WORK/2"
for spec in swipe:10 hold:15; do n=${spec%:*}; c=${spec#*:}; d="$S/$n"; mkdir -p "$d"
magick "$H/hand-$n-sheet.webp" -crop 248x405 +repage "$d/t_%03d.png"
magick montage -font "$FONT" "$d"/t_*.png -tile ${c}x -geometry 248x405+8+8 -background none "$d/sheet.png"
magick "$d/sheet.png" -quality 90 -define webp:alpha-quality=100 -define webp:method=6 "$d/sheet.webp"
echo $n $(ls "$d"/t_*.png | wc -l) $(magick identify -format "%wx%h" "$d/sheet.webp") $(stat -f %z "$d/sheet.webp"); done
mkdir -p "$WORK/3/out"
cp "$S/swipe/sheet.webp" "$WORK/3/out/hand-swipe-sheet.webp"; cp "$S/hold/sheet.webp" "$WORK/3/out/hand-hold-sheet.webp"

# 0.75 scale in 20 columns keeps both sheets within 4096 px a side.
S="$WORK/4"; H="$WORK/3/out"
for spec in swipe:10:100 hold:15:210; do n=${spec%%:*}; rest=${spec#*:}; c=${rest%%:*}; total=${rest#*:}; d="$S/$n"; mkdir -p "$d"
for i in $(seq 0 $((total-1))); do x=$(( (i%c)*264+8 )); y=$(( (i/c)*421+8 )); magick "$H/hand-$n-sheet.webp" -crop 248x405+$x+$y +repage -resize 186x304\! "$d/t_$(printf %03d $i).png"; done
magick montage -font "$FONT" "$d"/t_*.png -tile 20x -geometry 186x304+8+8 -background none "$d/sheet.png"
magick "$d/sheet.png" -quality 90 -define webp:alpha-quality=100 -define webp:method=6 "$d/sheet.webp"
echo $n $(ls "$d"/t_*.png | wc -l) $(magick identify -format "%wx%h" "$d/sheet.webp") $(stat -f %z "$d/sheet.webp"); done

cp "$S/swipe/sheet.webp" "$DEST/hand-swipe-sheet.webp"; cp "$S/hold/sheet.webp" "$DEST/hand-hold-sheet.webp"
