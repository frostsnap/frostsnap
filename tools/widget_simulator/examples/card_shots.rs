//! Render the device screens of the 64-word split card to PNG, headless: no window and
//! no device. Each widget is driven the way the firmware drives it (touches, swipes and draws
//! over time), on a 240x280 display with the device palette and fonts. The file names carry
//! the design's screen ids (D1-D9, R1-R4).
//!
//! ```sh
//! cargo run -p widget_simulator --example card_shots -- <out_dir>
//! ```
//!
//! It never opens a window, but the package links SDL2, so the linker needs `libSDL2.so`.
#![cfg(not(target_arch = "riscv32"))]

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay};
use frostsnap_core::{message::HeldShare2, schnorr_fun::frost::SecretShare};
use frostsnap_widgets::{
    CardHomeLine, DynWidget, HOLD_TO_CONFIRM_TIME_MS, Instant, Standby, SuperDrawTarget, Widget,
    backup::{BackupDisplay, CheckBackupScreen},
    card::*,
    demo_widget::card_demos::demo_card_words,
    palette::PALETTE,
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

const SCREEN: Size = Size::new(240, 280);
const FRAME_MS: u64 = 16;
/// The middle of the hold circle.
const HOLD: Point = Point::new(120, 215);
/// The middle of the full-width button.
const BUTTON: Point = Point::new(120, 253);
/// The first quiz option.
const FIRST_OPTION: Point = Point::new(120, 98);

struct Sim<W> {
    w: W,
    display: Rc<RefCell<SimulatorDisplay<Rgb565>>>,
    t: u64,
}

impl<W: Widget<Color = Rgb565>> Sim<W> {
    fn new(mut w: W) -> Self {
        w.set_constraints(SCREEN);
        let mut display = SimulatorDisplay::new(SCREEN);
        display.clear(PALETTE.background).unwrap();
        let mut sim = Self {
            w,
            display: Rc::new(RefCell::new(display)),
            t: 1_000,
        };
        sim.run(600);
        sim
    }

    fn frame(&mut self) {
        let mut target = SuperDrawTarget::from_shared(self.display.clone(), PALETTE.background);
        let now = Instant::from_millis(self.t);
        let _ = self.w.draw(&mut target, now);
    }

    fn run(&mut self, ms: u64) {
        let end = self.t + ms;
        while self.t < end {
            self.t = (self.t + FRAME_MS).min(end);
            self.frame();
        }
    }

    fn tap(&mut self, p: Point) {
        self.w.handle_touch(p, Instant::from_millis(self.t), false);
        self.run(60);
        self.w.handle_touch(p, Instant::from_millis(self.t), true);
    }

    fn hold(&mut self, p: Point, ms: u64) {
        self.w.handle_touch(p, Instant::from_millis(self.t), false);
        self.run(ms);
        self.w.handle_touch(p, Instant::from_millis(self.t), true);
    }

    fn drag(&mut self, ys: &[u32]) {
        self.w.handle_touch(
            Point::new(120, ys[0] as i32),
            Instant::from_millis(self.t),
            false,
        );
        let mut prev = ys[0];
        for &y in &ys[1..] {
            self.run(FRAME_MS);
            self.w.handle_vertical_drag(Some(prev), y, false);
            prev = y;
        }
        self.w.handle_vertical_drag(Some(prev), prev, true);
        self.w.handle_touch(
            Point::new(120, prev as i32),
            Instant::from_millis(self.t),
            true,
        );
    }

    fn swipe_up(&mut self) {
        self.drag(&[200, 170, 140, 110]);
    }
}

struct Shots {
    dir: PathBuf,
    n: usize,
}

impl Shots {
    fn take<W>(&mut self, sim: &Sim<W>, name: &str) {
        self.n += 1;
        let file = self.dir.join(format!("{:02}_{name}.png", self.n));
        sim.display
            .borrow()
            .to_rgb_output_image(&OutputSettingsBuilder::new().build())
            .save_png(&file)
            .expect("save png");
        println!("{}", file.display());
    }

    fn page(&mut self, page: CardInfoPage, name: &str) {
        let sim = Sim::new(page);
        self.take(&sim, name);
    }
}

fn main() {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: card_shots <out_dir>"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let mut shots = Shots { dir, n: 0 };
    let card = demo_card_words();
    let device_half: [u16; 32] = card[..32].try_into().unwrap();

    // D1
    for ((title, status), name) in [
        (BUSY_PREPARING_CARD, "D1_preparing_first_time"),
        (BUSY_OPENING_CARD, "D1_preparing_resume"),
    ] {
        let mut busy = CardBusyScreen::new(title, status);
        busy.set_progress(0.4);
        shots.take(&Sim::new(busy), name);
    }
    shots.page(COPY_WAITING_FOR_PHONE.page(), "D1b_waiting_for_phone");

    // D2: words 1-32 on the standard backup display: 11 pages of words, all 32, then the hold
    let mut sim = Sim::new(BackupDisplay::card_half(device_half));
    sim.run(1_000);
    shots.take(&sim, "D2_words_1_to_3");
    for page in 2..=11 {
        sim.swipe_up();
        sim.run(1_000);
        if page == 11 {
            shots.take(&sim, "D2_words_31_32");
        }
    }
    sim.swipe_up();
    sim.run(1_000);
    shots.take(&sim, "D2b_all_words_1_32");
    sim.swipe_up();
    sim.run(1_500);
    shots.take(&sim, "D2c_check_you_have_written");
    sim.hold(HOLD, HOLD_TO_CONFIRM_TIME_MS as u64 + 200);
    sim.run(1_500);
    shots.take(&sim, "D2c_now_lets_check_them");

    // D3: the quiz on words 1-32, retry until right
    let items: Vec<(u8, u16)> = (0..32u8)
        .map(|j| (j + 1, device_half[j as usize]))
        .collect();
    let mut sim = Sim::new(CheckBackupScreen::for_words(
        items.clone(),
        "Words 1-32 checked",
        0x5eed,
    ));
    sim.run(800);
    shots.take(&sim, "D3_select_word_1");
    sim.tap(FIRST_OPTION);
    sim.run(150);
    shots.take(&sim, "D3_a_pick_right_green_or_wrong_red");
    shots.take(
        &Sim::new(CardSuccess::new("Words 1-32 checked")),
        "D3_success_words_1_32_checked",
    );

    // D4, then the proofs while words 33-64 are written in the app (owner B1, B2)
    shots.page(COPY_PHONE_HALF_NEXT.page(), "D4_now_use_your_phone");
    let (title, status) = BUSY_PROVING;
    let mut busy = CardBusyScreen::new(title, status);
    busy.set_progress(0.4);
    shots.take(&Sim::new(busy), "D4b_use_your_phone_proving");

    // D5 to D7: the device round, then the phone round's gate
    shots.page(COPY_CARD_FINISHED_WAITING.page(), "D5a_look_at_your_phone");
    shots.page(
        COPY_DEVICE_ROUND_INTRO.page(),
        "D5_last_check_on_this_device",
    );
    // TOMORROW / TOMATO / TONIGHT, then DOCUMENT / DOLPHIN / DOMAIN
    let mut sim = Sim::new(DeviceRoundAnswer::new(44, 0, [1825, 1824, 1828], 20));
    shots.take(&sim, "D6_select_word_45");
    sim.tap(OPTION_RECTS[1].center());
    sim.run(500);
    shots.take(&sim, "D6_flash_tap_to_undo");
    sim.w.set_question(63, 4, [515, 518, 519], 21);
    sim.run(600);
    shots.take(&sim, "D6_select_word_64");
    shots.page(device_round_mismatch_page(44), "D6b_look_again_at_word_45");
    let mut gate = Sim::new(GateScreen::new(2, false));
    gate.run(100);
    shots.take(&gate, "D7_answer_on_your_phone");
    let mut gate = Sim::new(GateScreen::new(0, true));
    gate.run(100);
    shots.take(&gate, "D7_preparing");

    // D8
    let mut sim = Sim::new(CardSuccess::new("Backup checked"));
    sim.run(1_200);
    shots.take(&sim, "D8_backup_checked");
    for (copy, name) in [
        (COPY_CARD_DONE, "D8_card_done"),
        (COPY_DEVICE_ROUND_FAILED, "D8_alarm_device_round_failed"),
        (
            COPY_DEVICE_ROUND_NEXT_STEP,
            "D8_alarm_device_round_next_step",
        ),
        (COPY_PHONE_ROUND_FAILED, "D8_alarm_phone_round_failed"),
        (COPY_NEXT_STEP, "D8_alarm_next_step"),
        (COPY_STOPPED, "D8_check_stopped"),
        (COPY_NEXT_STEP_BEFORE_N, "D8_stopped_next_step"),
        (COPY_REDISPLAY_REFUSED, "D8_card_is_finished"),
        (COPY_REFUSED_RUN_CAP, "D8_refused_run_cap"),
        (COPY_REFUSED_THRESHOLD_ONE, "D8_refused_threshold_one"),
        (COPY_REFUSED_NOT_KEYGEN, "D8_refused_not_keygen"),
        (COPY_REFUSED_NOT_CARD_KEY, "D8_refused_not_card_key"),
        (COPY_REFUSED_NOT_DIRECT_USB, "D8_refused_not_direct_usb"),
        (COPY_DEVICE_ERROR, "D8_device_error"),
    ] {
        shots.page(copy.page(), name);
    }

    // D9: a later check, words 1-32 then the device round's 5 words of 33-64
    let mut recheck = items;
    recheck.extend([40u8, 45, 52, 58, 63].map(|j| (j + 1, card[j as usize])));
    let mut sim = Sim::new(CheckBackupScreen::for_words(recheck, "Backup checked", 7));
    sim.run(800);
    shots.take(&sim, "D9_recheck_37_items");

    // R1: the entry of words 1-32, then the review and the hold "Done"
    let mut sim = Sim::new(CardRestoreEntry::new());
    sim.run(600);
    shots.take(&sim, "R1_word_1_keyboard");
    let mut entries: [Option<u16>; 32] = device_half.map(Some);
    entries[4] = None;
    sim.w.review(entries);
    sim.run(1_000);
    shots.take(&sim, "R1_review_words_1_3");
    sim.swipe_up();
    sim.run(1_000);
    shots.take(&sim, "R1_review_words_4_6_unreadable");
    for _ in 2..11 {
        sim.swipe_up();
        sim.run(1_000);
    }
    shots.take(&sim, "R1_review_words_31_32");
    sim.swipe_up();
    sim.run(1_200);
    shots.take(&sim, "R1_hold_done");

    // R2 to R4
    shots.page(
        COPY_WAITING_FOR_PHONE_HALF.page(),
        "R2_enter_words_33_64_in_the_app",
    );
    let (title, status) = BUSY_CHECKING_CARD;
    let mut busy = CardBusyScreen::new(title, status);
    busy.set_progress(0.6);
    shots.take(&Sim::new(busy), "R3_checking_your_card");
    shots.page(key_restored_page(2, Some(2), true), "R4_key_restored");
    shots.page(
        threshold_warning_page(Some(3), Some(2)),
        "R4_threshold_differs",
    );
    shots.page(
        threshold_warning_page(None, Some(2)),
        "R4_threshold_unreadable",
    );
    shots.page(
        threshold_hold_page(Some(3), 2),
        "C1_consolidate_threshold_hold",
    );
    shots.page(
        words_didnt_fit_page((1 << 11) | (1 << 39)),
        "R4_words_12_and_40_didnt_fit",
    );
    shots.page(words_unreadable_page(1 << 4), "R4_word_5_unreadable");
    for (copy, name) in [
        (COPY_RECORD_DIFFERS, "R4_wallet_record_not_confirmed"),
        (COPY_HOLD_TO_RESTORE, "R4_hold_to_restore"),
        (COPY_RESTORE_FAILED, "R4_couldnt_read_the_card"),
        (COPY_TOO_MANY_UNREADABLE, "R4_too_many_unreadable"),
        (COPY_NEEDS_NEWER_FIRMWARE, "R4_needs_newer_firmware"),
        (COPY_ALREADY_HELD, "R4_already_held"),
        (COPY_WRONG_WALLET, "R4_wrong_wallet"),
    ] {
        shots.page(copy.page(), name);
    }
    let mut sim = Sim::new(COPY_RESTORE_FAILED.page());
    sim.w
        .handle_touch(BUTTON, Instant::from_millis(sim.t), false);
    sim.run(40);
    shots.take(&sim, "R4_edit_words_1_32_pressed");

    // the home line
    for line in [CardHomeLine::NotChecked, CardHomeLine::Failed] {
        let mut standby = Standby::new("0.4.0");
        let share_image = {
            use frostsnap_core::schnorr_fun::fun::prelude::*;
            SecretShare {
                index: Scalar::<Public, NonZero>::one(),
                share: Scalar::<Secret, Zero>::zero(),
            }
            .share_image()
        };
        standby.set_key_with_card_line(
            "Alice",
            HeldShare2 {
                access_structure_ref: Some(frostsnap_core::AccessStructureRef {
                    key_id: frostsnap_core::KeyId([0u8; 32]),
                    access_structure_id: frostsnap_core::AccessStructureId([1u8; 32]),
                }),
                share_image,
                threshold: Some(2),
                key_name: Some("Family Wallet".to_string()),
                purpose: None,
                needs_consolidation: false,
            },
            Some(line),
        );
        let mut sim = Sim::new(standby);
        sim.run(2_000);
        shots.take(&sim, &format!("home_{line:?}"));
    }
}
