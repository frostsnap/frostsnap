# Hand gestures

The frosted hand that swipes and holds over the device in the device action dialogs.

```
hand still ─ make_keys.sh ─▶ key images ─ wan_*.py ─▶ Wan clips ─ extract_hand.py ─▶ RGBA frames
  ─ pack_sheets.sh ─▶ source/hand-*-sheet.webp ─ just hand-gestures ─▶ app clips + manifest
```

Everything before the sheets was a one-off run on 30 Oct 2026, with the sheets repacked on 9 Nov.
The scripts in `pipeline/` are those runs, recovered at the revisions that ran. Their working files
are gone; see [Missing](#missing).

## Building the app clips

`just hand-gestures` runs `cargo run --release -p hand_gestures` (`src/main.rs`). It reads
`source/hand-swipe-sheet.webp` and `source/hand-hold-sheet.webp`, whose cells are 186×304, 20 to a
row, with 8 px of transparency round each. It writes:

- `frostsnapp/assets/devices/hand/{swipe,tap,hold-press,hold-release}.webp`, animated lossy WebP
  at quality 80 and 30 fps;
- `frostsnapp/rust/src/device_demo/hand_gestures.rs`, the clips' contact and lift times, which the
  device demo schedules its touches by.

The clips are cut from source frames as follows.

| Clip | Frames |
| --- | --- |
| swipe | swipe frames 0–94; the stroke, 30–55, plays at double speed |
| hold-press | hold frames 0–90; frame 90 is held while the hold lasts |
| hold-release | hold frames 91–209 |
| tap | every other hold frame from 20 to 44, frame 45 three times, then every other frame from 137 to 171 |

Contact is at swipe frame 30 and hold frame 45. The lift is at hold frame 137. The builder checks
that each encoded clip starts a frame at those times and ends at rest.

## How the sheets were made

1. **Still.** `source/hand-still-frosted.png`, 1254×1254 RGBA, from Codex's built-in `image_gen`.
   The generation prompt and the cleanup edit prompt are the `abstract-frosted-02` entry of
   `abstract_variants` in `pipeline/prompts.json`.
2. **Key images.** `pipeline/make_keys.sh <out> <swipe-up screen> <hold-to-confirm screen>`. It
   composites the still over the blue device plugged into the phone, scaled to 0.8 (1003 px) with
   its alpha multiplied by 0.6, the fingertip resting just below the device, cropped to 900×1250.
   The swipe key has the swipe-up screen on the device and the hold key has the hold-to-confirm
   screen. Outside the screen the two keys are identical, so both clips start and end on the same
   rest pose. It also writes the hand-free plates `key-nohand.png` and `base-hold-crop.png`.
3. **Wan clips.** `pipeline/wan_swipe.py` and `pipeline/wan_hold.py` submit a key to fal.ai
   `alibaba/wan-3.0/image-to-video` and download the result. The prompts are in the scripts.
   `wan_hold.py` was made by copying `wan_swipe.py`, so it still carries the swipe prompt as
   `prompt_unused`. The fal key is read from a `FAL_AI_KEY=` line in the env file that
   `FAL_ENV_FILE` names. Settings: 720p, adaptive aspect, audio off, prompt expansion off,
   `DURATION=7`, and `END_IMAGE` set to the key itself so the clip ends where it starts:

   ```sh
   FAL_ENV_FILE=.env DURATION=7 END_IMAGE=key-frosted-0.8.png \
     python3 -I wan_swipe.py key-frosted-0.8.png swipe.mp4 "The translucent frosted white hand silhouette"
   FAL_ENV_FILE=.env DURATION=7 END_IMAGE=key-hold-frosted-0.8.png \
     python3 -I wan_hold.py key-hold-frosted-0.8.png hold.mp4 "The translucent frosted white hand silhouette"
   ```

   | Clip | fal request id | Seed |
   | --- | --- | --- |
   | swipe | `01a18b49-b159-7062-ae7b-948bee88755a` | 617001765 |
   | hold | `01a18b5b-453b-7221-872a-3de3724550c6` | 190219203 |

   The scripts send no seed; these are the seeds fal reported. Each clip is 814×1130, 30 fps,
   210 frames.
4. **Cut-out.** `pipeline/extract_hand.py <clip> <plate> <out dir>`, with SAM 2.1
   (`facebook/sam2.1-hiera-large`, through `transformers`' `Sam2VideoModel`). It ran in a uv venv:
   `uv venv --python 3.12` then `uv pip install torch torchvision transformers pillow numpy`
   (transformers 5.22.0 at the time), with `HF_HUB_OFFLINE=1` against a cached model. SAM is
   seeded on frame 0 with positive points (393,712), (430,800), (480,900), (540,1020) on the hand
   and negative points (395,450), (395,150) on the device. Each pixel is un-blended against the
   plate, `colour = (f − bg·0.4) / 0.6`, and `alpha = 0.6 · clip((mask − 0.3) / 0.4)`. The swipe
   clip pairs with `key-nohand.png` and the hold clip with `base-hold-crop.png`. Each run writes
   210 RGBA PNGs at 814×1130 into `hand-swipe/` or `hand-hold/`.
5. **Sheets.** `pipeline/pack_sheets.sh <dir holding hand-swipe/ and hand-hold/> <work dir>` runs
   the four packing passes in order and copies the result into `source/`. It needs Pillow and
   ImageMagick; `MONTAGE_FONT` names any TTF if Arial isn't at its macOS path:
   1. `first_pack.py` crops every frame to the hand's union box (317,319)–(814,1130) and scales it
      with Lanczos to 248×405, 10 cells to a row. It writes WebP at quality 82 with alpha
      quality 90. The swipe is frames 0–94, one swipe from rest to rest. The hold is all 210.
   2. The hold sheet is repacked 15 wide (`magick montage`, quality 90).
   3. Every cell gets an 8 px transparent margin (quality 90, alpha quality 100).
   4. Every cell is scaled to 186×304 and packed 20 wide.

   Passes 3 and 4, run on the 248×405 sheets in commit `6e52dff71`
   (`frostsnapp/assets/devices/hand/`), reproduce the checked-in sheets byte for byte.
6. **App clips.** `just hand-gestures`.

## Inputs, and how to remake them

| Input | Where it comes from |
| --- | --- |
| The hand still | `source/hand-still-frosted.png`. |
| The blue device and the phone shell | `frostsnapp/assets/devices/topdown/device-blue.webp` and `phone.webp`; see `tools/topdown_art/README.md`. |
| The phone's dark panel | Drawn by `make_keys.sh`. |
| The swipe-up and hold-to-confirm screens | `widget_simulator` recordings of `sign_prompt`; not checked in. See below. |
| `key-nohand.png`, `base-hold-crop.png` | Written by `make_keys.sh`. |
| The Wan clips | `source/swipe-wan.mp4` and `source/hold-wan.mp4`. These are H.264 (CRF 22) re-encodes; `extract_hand.py` ran on the originals. |

The screens came from a recording at `450fefa40`, with the simulator's touch marker hidden and
the demo transaction changed to one recipient of 250,284 sats, a fee of 284 sats and 2.3 sat/vB.
`sign_screen_patch.py` makes both edits. It edits tracked files, so run it in a throwaway worktree:

```sh
(
  set -euo pipefail
  WT=$(mktemp -d)/sign-screens
  git worktree add --detach "$WT" 450fefa40
  trap 'git worktree remove --force "$WT"' EXIT
  python3 tools/hand_gestures/pipeline/sign_screen_patch.py "$WT"
  (cd "$WT" && LIBRARY_PATH="$(brew --prefix sdl2-compat)/lib" cargo build -p widget_simulator)
  mkdir -p sign && python3 tools/hand_gestures/pipeline/sign_screen_cmds.py sign > sign.cmds
  "$WT/target/debug/widget_simulator" sign_prompt --timeout 150000 < sign.cmds
  magick sign/f0040.png -region 240x232+0+0 -blur 0x7 +region swipe-screen.png
  cp sign/f0235.png hold-screen.png
)
```

The blur hides the placeholder data and leaves the "Swipe up" hint sharp. This hasn't been rerun, so
it's not checked that the frames match the originals pixel for pixel.

## Adding a gesture

1. **Compose a key.** Put the new screen on the device in `make_keys.sh` and keep the hand's
   placement. The difference check at the end of the script must report `count=0`, so the new
   clip starts on the same rest pose as the others.
2. **Run Wan.** Copy `wan_hold.py`, describe the gesture in its prompt, and run it with
   `END_IMAGE` set to the key so it ends at rest.
3. **Cut out.** Run `extract_hand.py` on the clip with that key's hand-free plate.
4. **Pack.** Run `first_pack.py`'s crop and scale, then the passes in `pack_sheets.sh`. The
   builder assumes 186×304 cells, 20 to a row, with an 8 px margin. Keep the same crop box, so
   `FINGERTIP` still holds.
5. **Add it to the builder.** Add a `Clip` in `src/main.rs`, with a `Sheet` variant if it has its
   own sheet. Emit its contact times into the manifest, then run `just hand-gestures`.

## Missing

- **The lossless RGBA frames** that `extract_hand.py` wrote. They were in a deleted scratch
  directory. The sheets are the only copy, after four lossy WebP passes.
- **The original Wan mp4s.** They were deleted locally. fal may still hold them under the request
  ids above; this has not been checked. `source/*-wan.mp4` are the best surviving copies.
- **The device screens** that went into the key images. They can only be remade approximately
  (above).
- **The SAM venv's exact versions**, apart from transformers 5.22.0.
- **The first pass's 10-column hold sheet.** It was overwritten by pass 2. The pass 2 hold sheet
  and the first pass's swipe sheet are in commit `6e52dff71`.
