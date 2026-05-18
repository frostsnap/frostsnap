//! ESP32-S3 QEMU virtual touch backend.
//!
//! This expects Frostsnap's patched Espressif QEMU `display.esp.rgb` device to
//! expose a small touch register block immediately after the stock RGB panel
//! registers at `0x2100_0000`.

use core::ptr;

use frostsnap_cst816s::{interrupt, TouchEvent, TouchGesture};

use crate::qemu_display::{HEIGHT, WIDTH};

const RGB_QEMU_BASE: usize = 0x2100_0000;
const TOUCH_POS_OFFSET: usize = 0x1c;
const TOUCH_STATE_OFFSET: usize = 0x20;
const TOUCH_ACK_OFFSET: usize = 0x24;

const TOUCH_PENDING: u32 = 1 << 0;
const TOUCH_PRESSED: u32 = 1 << 1;
const TOUCH_ACTION_SHIFT: u32 = 8;
const TOUCH_ACTION_MASK: u32 = 0xff << TOUCH_ACTION_SHIFT;

pub fn init() {}

pub fn poll() {
    let state = unsafe { ptr::read_volatile(reg(TOUCH_STATE_OFFSET)) };
    if state & TOUCH_PENDING == 0 {
        return;
    }

    let pos = unsafe { ptr::read_volatile(reg(TOUCH_POS_OFFSET)) };
    let action = ((state & TOUCH_ACTION_MASK) >> TOUCH_ACTION_SHIFT) as u8;
    let x = ((pos >> 16) as i32).clamp(0, WIDTH as i32 - 1);
    let y = ((pos & 0xffff) as i32).clamp(0, HEIGHT as i32 - 1);
    let pressed = state & TOUCH_PRESSED != 0;

    interrupt::enqueue_virtual(TouchEvent {
        x,
        y,
        gesture: TouchGesture::None,
        action,
        finger_id: 0,
        pressure: if pressed { 128 } else { 0 },
        area: if pressed { 8 } else { 0 },
    });

    unsafe {
        ptr::write_volatile(reg(TOUCH_ACK_OFFSET), TOUCH_PENDING);
    }
}

fn reg(offset: usize) -> *mut u32 {
    (RGB_QEMU_BASE + offset) as *mut u32
}
