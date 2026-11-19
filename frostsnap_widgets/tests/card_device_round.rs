//! The device round's answer screen (D6, on words 33-64): the same flash and timing for every
//! option, no style that depends on the option, and no Stop during the flash.
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use frostsnap_widgets::{
    card::{DeviceRoundAnswer, DeviceRoundEvent, FLASH_MS, OPTION_RECTS, UNDO_CAP_PER_BOX},
    palette::PALETTE,
    vec_framebuffer::VecFramebuffer,
    DynWidget, Instant, SuperDrawTarget, Widget,
};
use std::{cell::RefCell, rc::Rc};

const SCREEN: Size = Size::new(240, 280);
const FRAME_MS: u64 = 25;
// TOMORROW, TOMATO, TONIGHT
const OPTIONS: [u16; 3] = [1825, 1824, 1828];

struct Sim {
    answer: DeviceRoundAnswer,
    fb: Rc<RefCell<VecFramebuffer<Rgb565>>>,
    t: u64,
}

impl Sim {
    fn new() -> Self {
        let mut answer = DeviceRoundAnswer::new(44, 2, OPTIONS, 31);
        answer.set_constraints(SCREEN);
        let mut fb = VecFramebuffer::new(SCREEN.width as usize, SCREEN.height as usize);
        fb.clear(PALETTE.background);
        let mut sim = Self {
            answer,
            fb: Rc::new(RefCell::new(fb)),
            t: 1_000,
        };
        sim.run(500);
        sim
    }

    fn frame(&mut self) {
        let mut target = SuperDrawTarget::from_shared(self.fb.clone(), PALETTE.background);
        let now = Instant::from_millis(self.t);
        self.answer.draw(&mut target, now).unwrap();
    }

    fn run(&mut self, ms: u64) {
        let end = self.t + ms;
        while self.t < end {
            self.t += FRAME_MS;
            self.frame();
        }
    }

    fn tap(&mut self, p: Point) {
        self.answer
            .handle_touch(p, Instant::from_millis(self.t), false);
        self.t += 40;
        self.answer
            .handle_touch(p, Instant::from_millis(self.t), true);
    }

    fn swipe_down(&mut self) {
        self.answer.handle_vertical_drag(Some(100), 140, false);
        self.answer.handle_vertical_drag(Some(140), 200, true);
        self.answer
            .handle_touch(Point::new(120, 200), Instant::from_millis(self.t), true);
    }

    /// Frames from the tap until the pick, with the screen 600 ms in.
    fn until_event(&mut self) -> (u64, Option<DeviceRoundEvent>, Vec<u8>) {
        let released = self.t;
        let mut at_600 = Vec::new();
        for _ in 0..200 {
            self.t += FRAME_MS;
            self.frame();
            if self.t - released == 600 {
                at_600 = self.fb.borrow().data().to_vec();
            }
            if let Some(event) = self.answer.take_event() {
                return (self.t - released, Some(event), at_600);
            }
        }
        (0, None, at_600)
    }
}

fn pick(i: usize) -> (u64, Option<DeviceRoundEvent>, Vec<u8>) {
    let mut sim = Sim::new();
    sim.tap(OPTION_RECTS[i].center());
    assert!(sim.answer.is_flashing());
    sim.until_event()
}

#[test]
fn every_option_gets_the_same_flash_and_timing() {
    let results: Vec<_> = (0..4).map(pick).collect();
    for (i, (tick, event, _)) in results.iter().enumerate() {
        assert!((FLASH_MS..FLASH_MS + FRAME_MS).contains(tick));
        assert_eq!(
            *tick, results[0].0,
            "option {i}: the pick came at another time"
        );
        let expected = (i < 3).then_some(i as u8);
        assert_eq!(
            *event,
            Some(DeviceRoundEvent::Pick {
                display_id: 31,
                pick: expected
            })
        );
    }

    // mid-flash, two picks' screens differ only inside the two buttons tapped: no option
    // is styled as right or wrong
    for a in 0..4 {
        for b in 0..4 {
            let (fa, fb) = (&results[a].2, &results[b].2);
            for y in 0..SCREEN.height as i32 {
                for x in 0..SCREEN.width as i32 {
                    let p = Point::new(x, y);
                    if OPTION_RECTS[a].contains(p) || OPTION_RECTS[b].contains(p) {
                        continue;
                    }
                    let i = (y as usize * SCREEN.width as usize + x as usize) * 2;
                    assert_eq!(
                        fa[i..i + 2],
                        fb[i..i + 2],
                        "picks {a} and {b} differ at {p}"
                    );
                }
            }
        }
    }

    // and the flash marks each button the same way: the edge of the tapped button looks the
    // same whichever button it is
    let reference: Vec<Vec<u8>> = (0..4)
        .map(|i| {
            let rect = OPTION_RECTS[i];
            let frame = &results[i].2;
            let mut edge = Vec::new();
            for dy in 0..rect.size.height as i32 {
                for dx in 0..rect.size.width as i32 {
                    if dx >= 3
                        && dx < rect.size.width as i32 - 3
                        && dy >= 3
                        && dy < rect.size.height as i32 - 3
                    {
                        continue;
                    }
                    let p = rect.top_left + Point::new(dx, dy);
                    let k = (p.y as usize * SCREEN.width as usize + p.x as usize) * 2;
                    edge.extend_from_slice(&frame[k..k + 2]);
                }
            }
            edge
        })
        .collect();
    for i in 1..4 {
        assert_eq!(
            reference[i], reference[0],
            "button {i} is flashed differently"
        );
    }
}

/// Owner B12: the device round runs after N, so a swipe down never asks to stop: not while
/// answering, not in the flash (where the pick still goes through), not after the pick.
#[test]
fn a_swipe_down_never_stops() {
    let mut sim = Sim::new();
    sim.run(300);
    sim.swipe_down();
    assert_eq!(sim.answer.take_event(), None);
    sim.tap(OPTION_RECTS[1].center());
    sim.run(300);
    sim.swipe_down();
    assert_eq!(sim.answer.take_event(), None);
    let (_, event, _) = sim.until_event();
    assert!(matches!(event, Some(DeviceRoundEvent::Pick { .. })));
    sim.swipe_down();
    assert_eq!(sim.answer.take_event(), None);
}

#[test]
fn undo_goes_back_and_is_capped() {
    let mut sim = Sim::new();
    for used in 1..=UNDO_CAP_PER_BOX {
        sim.tap(OPTION_RECTS[0].center());
        sim.run(400);
        sim.tap(Point::new(120, 150));
        sim.run(400);
        assert!(!sim.answer.is_flashing(), "undo {used} goes back");
        assert_eq!(sim.answer.undos_left(), UNDO_CAP_PER_BOX - used);
        assert_eq!(sim.answer.take_event(), None);
    }
    sim.tap(OPTION_RECTS[3].center());
    sim.run(400);
    sim.tap(Point::new(120, 150));
    let (_, event, _) = sim.until_event();
    assert_eq!(
        event,
        Some(DeviceRoundEvent::Pick {
            display_id: 31,
            pick: None
        })
    );

    // the second look at the same word keeps the budget; the next word gets a fresh one
    sim.answer.set_question(44, 2, [1828, 1825, 1824], 32);
    assert_eq!(sim.answer.undos_left(), 0);
    sim.answer.set_question(52, 3, OPTIONS, 33);
    assert_eq!(sim.answer.undos_left(), UNDO_CAP_PER_BOX);
}

#[test]
fn a_rebuilt_second_look_takes_the_undos_it_is_given() {
    // firmware that builds a new screen after the F4 page or a Stop passes what was left
    let mut sim = Sim::new();
    sim.answer = DeviceRoundAnswer::new(44, 2, OPTIONS, 31).with_undos_left(1);
    sim.answer.set_constraints(SCREEN);
    sim.run(500);
    sim.tap(OPTION_RECTS[1].center());
    sim.run(400);
    sim.tap(Point::new(120, 150));
    sim.run(400);
    assert!(!sim.answer.is_flashing());
    assert_eq!(sim.answer.undos_left(), 0);
    sim.tap(OPTION_RECTS[1].center());
    sim.run(400);
    sim.tap(Point::new(120, 150));
    let (_, event, _) = sim.until_event();
    assert_eq!(
        event,
        Some(DeviceRoundEvent::Pick {
            display_id: 31,
            pick: Some(1)
        })
    );
    let answer = DeviceRoundAnswer::new(44, 2, OPTIONS, 31).with_undos_left(9);
    assert_eq!(answer.undos_left(), UNDO_CAP_PER_BOX);
}

#[test]
fn picks_carry_the_display_they_were_made_on() {
    let mut sim = Sim::new();
    sim.answer.set_question(44, 2, [1828, 1825, 1824], 77);
    // in place: the grace starts once the new options are in, after the old ones' fade out
    // and the new ones' fade in
    sim.run(700);
    sim.tap(OPTION_RECTS[2].center());
    let (_, event, _) = sim.until_event();
    assert_eq!(
        event,
        Some(DeviceRoundEvent::Pick {
            display_id: 77,
            pick: Some(2)
        })
    );
    assert!(!format!("{event:?}").contains("Some(2)"));
}
