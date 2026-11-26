#!/usr/bin/env bash
# usage: make_keys.sh <out dir> <swipe-up screen.png> <hold-to-confirm screen.png>
#
# Writes the Wan key images key-frosted-0.8.png and key-hold-frosted-0.8.png, and the hand-free
# plates extract_hand.py un-blends against, key-nohand.png and base-hold-crop.png.
set -euo pipefail

REPO=$(cd "$(dirname "$0")/../../.." && pwd)
TOP=$REPO/frostsnapp/assets/devices/topdown
HAND=$REPO/tools/hand_gestures/source/hand-still-frosted.png
OUT=$1
SIGN_SCREEN=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
HOLD_SCREEN=$(cd "$(dirname "$3")" && pwd)/$(basename "$3")
mkdir -p "$OUT"
cd "$OUT"

Y0=1250; SC=1.77
dwebp -quiet "$TOP/device-blue.webp" -o dev.png; dwebp -quiet "$TOP/phone.webp" -o phone.png
magick "$SIGN_SCREEN" -resize 293x342! \( -size 293x342 xc:none -fill white -draw "roundrectangle 0,0 292,341 22,22" \) -compose CopyOpacity -composite screen.png
magick phone.png -resize 991x1982! phone991.png
python3 - <<EOF > geo.txt
S=1.77; Y0=1250
dx=279.5*S-179; dy=1011*S+3-84
print(round(dx), round(dy-Y0), round(dx+33), round(dy-Y0+132), round(70*S+18), round(109*S+18-Y0), round(419*S-36), round(902*S-36))
EOF
read DX DY SX SY PX PY PW PH < geo.txt; echo $DX $DY $SX $SY $PX $PY $PW $PH
magick -size ${PW}x${PH} gradient:'#0c1116'-'#030507' \( -size ${PW}x${PH} xc:none -fill white -draw "roundrectangle 0,0 $((PW-1)),$((PH-1)) 80,80" \) -compose CopyOpacity -composite panel.png

Y0=1450; H=2300; W0=2100; DX=316; DY=$(python3 -c "print(round(1011*1.77+3-84-$Y0))"); SX=349; SY=$((DY+132)); PX=142; PY=$(python3 -c "print(round(109*1.77+18-$Y0))")
magick -size ${W0}x$H xc:black dev.png -geometry +$DX+$DY -composite screen.png -geometry +$SX+$SY -composite panel.png -geometry +$PX$PY -composite phone991.png -geometry +0-$Y0 -composite base.png

TX=$((DX+179+40)); TY=$((DY+523+30))
key(){ name=$1; tipx=$2; tipy=$3; op=$4; k=$5
  W=$(python3 -c "print(round(1254*$k))"); ox=$(python3 -c "print(round($TX-$tipx*$k))"); oy=$(python3 -c "print(round($TY-$tipy*$k))")
  magick "$HAND" -resize ${W}x${W} -channel A -evaluate multiply $op +channel hand.png
  magick base.png hand.png -geometry +$ox+$oy -composite -crop 900x1250+100+40 +repage key-$name-$k.png; }
key abstract-frosted-02 425 256 0.6 0.8

magick base.png -crop 900x1250+100+40 +repage key-nohand.png
cp key-abstract-frosted-02-0.8.png key-frosted-0.8.png

magick "$HOLD_SCREEN" -resize 293x342! \( -size 293x342 xc:none -fill white -draw "roundrectangle 0,0 292,341 22,22" \) -compose CopyOpacity -composite screen-hold.png
magick -size ${W0}x$H xc:black dev.png -geometry +$DX+$DY -composite screen-hold.png -geometry +$SX+$SY -composite panel.png -geometry +$PX$PY -composite phone991.png -geometry +0-$Y0 -composite base-hold.png
key(){ name=$1; tipx=$2; tipy=$3; op=$4; out=$5; k=0.8
  W=$(python3 -c "print(round(1254*$k))"); ox=$(python3 -c "print(round($TX-$tipx*$k))"); oy=$(python3 -c "print(round($TY-$tipy*$k))")
  magick "$HAND" -resize ${W}x${W} -channel A -evaluate multiply $op +channel hand.png
  magick base-hold.png hand.png -geometry +$ox+$oy -composite -crop 900x1250+100+40 +repage "$out"; }
key abstract-frosted-02 425 256 0.6 key-hold-frosted-0.8.png

# Outside the device's display window the swipe and hold keys must be identical (count=0), or the
# two clips' rest poses won't match.
magick \( key-frosted-0.8.png -alpha off \) \( key-hold-frosted-0.8.png -alpha off \) -compose difference -composite -fill black -draw "rectangle 247,348 544,694" -grayscale Rec709Luma -threshold 0 -format "bbox=%@ count=%[fx:mean*w*h]\n" info:

magick base-hold.png -crop 900x1250+100+40 +repage base-hold-crop.png
