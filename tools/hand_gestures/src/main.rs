//! Builds the hand gestures the device action dialogs play: animated WebP clips cut from the
//! hand's source sprite sheets, and the Rust manifest the device demo schedules its touches from.
//! Run `just hand-gestures` from the repo root after changing either.

use std::{fmt::Write as _, fs, path::Path};

use image::{GenericImageView, RgbaImage};
use webp_animation::{Decoder, Encoder, EncoderOptions, EncodingConfig};

const FPS: u32 = 30;
const CELL: (u32, u32) = (186, 304);
const COLUMNS: u32 = 20;
// The sheets leave this much transparency around every cell, so filtering can't bleed one cell
// into the next.
const MARGIN: u32 = 8;
// Where the clips put the fingertip on the device's 240×280 screen, as they were framed.
const FINGERTIP: (i32, i32) = (120, 215);
const QUALITY: f32 = 80.0;

const SOURCE: &str = "tools/hand_gestures/source";
const ASSETS: &str = "frostsnapp/assets/devices/hand";
const MANIFEST: &str = "frostsnapp/rust/src/device_demo/hand_gestures.rs";

const SWIPE_FRAMES: u32 = 95;
const SWIPE_CONTACT: u32 = 30;
const SWIPE_STROKE_END: u32 = 55;
const HOLD_CONTACT: u32 = 45;
const HOLD_STILL: u32 = 90;
const HOLD_LIFT: u32 = 137;
const HOLD_FRAMES: u32 = 210;
// Hold frames up to this and from `HOLD_REST_AGAIN` are still the rest pose every clip starts and
// ends on.
const HOLD_REST_UNTIL: u32 = 20;
const HOLD_REST_AGAIN: u32 = 171;

#[derive(Clone, Copy)]
enum Sheet {
    Swipe,
    Hold,
}

struct Clip {
    file: &'static str,
    sheet: Sheet,
    frames: Vec<u32>,
    /// Frames until the hand is back at rest, which can run past the last distinct frame: the
    /// clip's final frame is the rest pose, and it is held.
    length: u32,
}

impl Clip {
    /// When `source_frame` shows in this clip.
    fn at(&self, source_frame: u32) -> u32 {
        let index = self
            .frames
            .iter()
            .position(|&f| f == source_frame)
            .unwrap_or_else(|| panic!("{} shows frame {source_frame}", self.file));
        ms(index as u32)
    }
}

fn ms(frames: u32) -> u32 {
    (frames as f64 * 1000.0 / FPS as f64).round() as u32
}

fn swipe() -> Clip {
    // The upward stroke plays at double speed, and the frames it saves are a longer rest at the
    // end, so a swipe still takes as long as the source's.
    Clip {
        file: "swipe.webp",
        sheet: Sheet::Swipe,
        frames: (0..SWIPE_CONTACT)
            .chain((SWIPE_CONTACT..=SWIPE_STROKE_END).step_by(2))
            .chain(SWIPE_STROKE_END + 1..SWIPE_FRAMES)
            .collect(),
        length: SWIPE_FRAMES,
    }
}

// A hold is split at its still frame, so a longer hold keeps that one frame up between the two
// clips instead of encoding it over and over.
fn hold_press() -> Clip {
    Clip {
        file: "hold-press.webp",
        sheet: Sheet::Hold,
        frames: (0..=HOLD_STILL).collect(),
        length: HOLD_STILL + 1,
    }
}

fn hold_release() -> Clip {
    Clip {
        file: "hold-release.webp",
        sheet: Sheet::Hold,
        frames: (HOLD_STILL + 1..HOLD_FRAMES).collect(),
        length: HOLD_FRAMES - HOLD_STILL - 1,
    }
}

// A tap is the hold's own reach, press and lift played quickly, from rest to rest.
fn tap() -> Clip {
    let frames: Vec<u32> = (HOLD_REST_UNTIL..HOLD_CONTACT)
        .step_by(2)
        .chain([HOLD_CONTACT; 3])
        .chain((HOLD_LIFT..=HOLD_REST_AGAIN).step_by(2))
        .collect();
    Clip {
        file: "tap.webp",
        sheet: Sheet::Hold,
        length: frames.len() as u32,
        frames,
    }
}

fn cell(sheet: &RgbaImage, index: u32) -> Vec<u8> {
    let x = (index % COLUMNS) * (CELL.0 + 2 * MARGIN) + MARGIN;
    let y = (index / COLUMNS) * (CELL.1 + 2 * MARGIN) + MARGIN;
    sheet.view(x, y, CELL.0, CELL.1).to_image().into_raw()
}

fn encode(clip: &Clip, sheet: &RgbaImage) -> Vec<u8> {
    let mut encoder = Encoder::new_with_options(
        CELL,
        EncoderOptions {
            encoding_config: Some(EncodingConfig::new_lossy(QUALITY)),
            ..Default::default()
        },
    )
    .expect("encoder");
    for (i, &frame) in clip.frames.iter().enumerate() {
        encoder
            .add_frame(&cell(sheet, frame), ms(i as u32) as i32)
            .expect("frame encodes");
    }
    encoder
        .finalize(ms(clip.length) as i32)
        .expect("clip encodes")
        .to_vec()
}

// The encoder merges identical neighbouring frames, so the clip's frames aren't the source's:
// what matters is that its times still start a frame at each point the demo times a touch by,
// and end at rest.
fn check(clip: &Clip, encoded: &[u8], points: &[u32]) {
    let decoder = Decoder::new(encoded).expect("clip decodes");
    assert_eq!(decoder.dimensions(), CELL, "{}", clip.file);
    let mut starts = vec![0];
    starts.extend(decoder.into_iter().map(|f| f.timestamp() as u32));
    assert_eq!(
        starts.last().copied(),
        Some(ms(clip.length)),
        "{}",
        clip.file
    );
    for point in points {
        assert!(
            starts.contains(point),
            "{} has no frame starting at {point} ms in {starts:?}",
            clip.file
        );
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let load = |file: &str| {
        image::open(root.join(SOURCE).join(file))
            .expect("source sheet")
            .to_rgba8()
    };
    let (swipe_sheet, hold_sheet) = (load("hand-swipe-sheet.webp"), load("hand-hold-sheet.webp"));
    let build = |clip: &Clip, points: &[u32]| -> String {
        let sheet = match clip.sheet {
            Sheet::Swipe => &swipe_sheet,
            Sheet::Hold => &hold_sheet,
        };
        let encoded = encode(clip, sheet);
        check(clip, &encoded, points);
        fs::write(root.join(ASSETS).join(clip.file), &encoded).expect("clip written");
        println!("{}: {} bytes", clip.file, encoded.len());
        format!(
            "Clip {{\n        asset: \"assets/devices/hand/{}\",\n        length_ms: {},\n    }}",
            clip.file,
            ms(clip.length)
        )
    };

    let (swipe, press, release, tap) = (swipe(), hold_press(), hold_release(), tap());
    let swipe_contact = swipe.at(SWIPE_CONTACT);
    let hold_contact = press.at(HOLD_CONTACT);
    let hold_lift = release.at(HOLD_LIFT);
    let (tap_contact, tap_lift) = (tap.at(HOLD_CONTACT), tap.at(HOLD_LIFT));

    let mut manifest = String::new();
    write!(
        manifest,
        "// Generated by `just hand-gestures` (tools/hand_gestures). Do not edit.\n\n\
         //! The hand clips' timing, which the demo's touches are scheduled by. Times are from the\n\
         //! start of the clip they fall in.\n\n\
         pub struct Clip {{\n    \
             pub asset: &'static str,\n    \
             /// Until the hand is back at rest. The clip's last frame shows from then on.\n    \
             pub length_ms: u32,\n\
         }}\n\n\
         pub struct Swipe {{\n    pub clip: Clip,\n    pub contact_ms: u32,\n}}\n\n\
         pub struct Tap {{\n    pub clip: Clip,\n    pub contact_ms: u32,\n    pub lift_ms: u32,\n}}\n\n\
         /// A hold's reach and press, then its lift. The press clip's last frame, finger down, is\n\
         /// held for as long as the hold lasts.\n\
         pub struct Hold {{\n    \
             pub press: Clip,\n    \
             pub contact_ms: u32,\n    \
             pub release: Clip,\n    \
             pub lift_ms: u32,\n\
         }}\n\n\
         pub const FPS: u32 = {FPS};\n\n\
         /// Where the clips put the fingertip on the device's screen.\n\
         pub const FINGERTIP: (i32, i32) = {FINGERTIP:?};\n\n\
         pub const SWIPE: Swipe = Swipe {{\n    clip: {},\n    contact_ms: {swipe_contact},\n}};\n\n\
         pub const TAP: Tap = Tap {{\n    clip: {},\n    contact_ms: {tap_contact},\n    \
             lift_ms: {tap_lift},\n}};\n\n\
         pub const HOLD: Hold = Hold {{\n    press: {},\n    contact_ms: {hold_contact},\n    \
             release: {},\n    lift_ms: {hold_lift},\n}};\n",
        build(&swipe, &[swipe_contact]),
        build(&tap, &[tap_contact, tap_lift]),
        build(&press, &[hold_contact]),
        build(&release, &[hold_lift]),
    )
    .expect("write");
    fs::write(root.join(MANIFEST), manifest).expect("manifest written");
}
