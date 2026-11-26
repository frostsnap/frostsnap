#!/usr/bin/env bash
# usage: devices.sh <out dir> [starting-set.webp]
#
# Writes device-{orange,blue,black,red,silver,plain}.webp as in frostsnapp/assets/devices/topdown/.
# Without a local starting-set.webp it fetches the website's.
set -euo pipefail

OUT=$(mkdir -p "$1" && cd "$1" && pwd)
SET=${2:-}
if [ -n "$SET" ]; then SET=$(cd "$(dirname "$SET")" && pwd)/$(basename "$SET"); fi
cd "$OUT"
if [ -n "$SET" ]; then cp "$SET" set.webp; else curl -fsSL https://aphanley.github.io/frostsnap-preview/assets/homepage-2026-09/starting-set.webp -o set.webp; fi
dwebp -quiet set.webp -o set.png

# The render's display is blanked to dark glass, because the app draws the device's live screen
# over it.
i=0; for c in orange blue black; do x=$((i*440)); magick set.png -crop 360x523+$x+0 +repage -fill '#0D1114' -draw "roundrectangle 28,132 330,474 22,22" raw-$c.png; i=$((i+1)); done
# Only the rim takes the new colour: the HSL saturation mask keeps the glass, plug and screen as
# they were.
rim(){ magick "$1" \( "$1" -modulate $2 \) \( "$1" -alpha off -colorspace HSL -channel G -separate +channel -level 18%,45% \) -composite \( "$1" -alpha extract \) -alpha off -compose CopyOpacity -composite "$3"; }
rim raw-orange.png 100,115,86 raw-red.png; rim raw-blue.png 175,0,100 raw-silver.png; rim raw-blue.png 95,0,100 raw-plain.png
for c in orange blue black red silver plain; do cwebp -quiet -q 88 -alpha_q 95 raw-$c.png -o device-$c.webp; done
