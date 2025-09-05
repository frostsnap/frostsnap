#![allow(dead_code)]
use esp_hal::{
    assist_debug::DebugAssist, macros::handler, peripherals::ASSIST_DEBUG,
    InterruptConfigurable as _,
};

extern "C" {
    static _stack_start: u32;
    static _stack_end: u32;
}

pub fn enable_stack_guard(assist_debug: &mut ASSIST_DEBUG) {
    let mut da = DebugAssist::new(assist_debug);
    da.set_interrupt_handler(interrupt_handler);
    let stack_top = unsafe { &_stack_start as *const u32 as u32 };
    let stack_bottom = unsafe { &_stack_end as *const u32 as u32 };
    da.enable_sp_monitor(stack_bottom, stack_top);
}

#[handler(priority = esp_hal::interrupt::Priority::min())]
fn interrupt_handler() {
    panic!("stack guard tripped");
}
