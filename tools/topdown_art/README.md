# Top-down device art

Where the images in `frostsnapp/assets/devices/topdown/` came from. All of them were made on
30 Oct 2026 and have not changed since.

| File | Source |
| --- | --- |
| `device-orange.webp`, `device-blue.webp`, `device-black.webp` | Crops of the website's `starting-set.webp` render, with the display blanked (`devices.sh`). |
| `device-red.webp` | A recolour of the orange crop's rim (`devices.sh`). |
| `device-silver.webp`, `device-plain.webp` | Recolours of the blue crop's rim (`devices.sh`). |
| `phone.webp` | Alex's `pixel-shell.webp` render, copied unchanged. |
| `laptop-edge.webp` | A Blender render from `laptop_top.py`. |

Silver, red and plain are recolours, not renders. Alex's own renders of those cases would replace
them.

## Devices

```sh
tools/topdown_art/devices.sh <out dir> [starting-set.webp]
```

Without a local copy of the render it fetches
`https://aphanley.github.io/frostsnap-preview/assets/homepage-2026-09/starting-set.webp`
(1238×523). The script crops `360x523+{0,440,880}+0` for orange, blue and black. It fills the
display with `#0D1114` and writes WebP at quality 88 with alpha quality 95. The rim recolours use
`-modulate` through an HSL-saturation mask: red is orange at `100,115,86`, silver is blue at
`175,0,100`, and plain is blue at `95,0,100`. Run on the website's render, it reproduces all six
checked-in files byte for byte.

## Phone

`phone.webp` is `pixel-shell.webp` from the website's `rendered/pixel9-pro-creation/` assets. It
is not in this repo.

## Laptop edge

`laptop_top.py` is a headless Blender script (run with Blender 5.2). It models a laptop base with
keys, a trackpad and a left USB-C port, and hides the lid. It renders the base straight down
through an orthographic camera (`ortho_scale` 180, 1500×1000, so 8.33 px/mm) with Cycles, 128
samples and denoising, the AgX view transform and a transparent film. It imports the device case
from `SFv2.7.3_3dp.stl` in the frostsnap-hardware repo, read from `~/src/frostsnap-hardware/`
unless `FROSTSNAP_CASE_STL` names it. The `base` layer then hides the device, so only the laptop
is in the picture. The script still builds the device, so it needs `device_front.py` and an
`outline.svg`, which `outline.py` cuts from the device outline in
`frostsnapp/lib/device_action_fullscreen_dialog.dart`.

```sh
cd tools/topdown_art
python3 outline.py
/Applications/Blender.app/Contents/MacOS/Blender -b --python laptop_top.py -- blue <out>/laptop-top-base 128 base
cwebp -quiet -q 86 -alpha_q 90 <out>/laptop-top-base.png -o laptop-edge.webp
```

Run again, this gives a render within 0.06% RMSE of the checked-in `laptop-edge.webp`. The
difference is Cycles noise.

No `.blend` file was saved. The scene exists only as this script.
