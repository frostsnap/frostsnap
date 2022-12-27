//! A simple example to change colours of a WS2812/NeoPixel compatible LED.
//!
//! This example demonstrates the use of [`FixedLengthSignal`][crate::rmt::FixedLengthSignal] which
//! lives on the stack and requires a known length before creating it.
//!
//! There is a similar implementation in the esp-idf project:
//! https://github.com/espressif/esp-idf/tree/20847eeb96/examples/peripherals/rmt/led_strip
//!
//! Datasheet (PDF) for a WS2812, which explains how the pulses are to be sent:
//! https://cdn-shop.adafruit.com/datasheets/WS2812.pdf

use anyhow::{bail, Result};
use core::time::Duration;
use esp_idf_hal::rmt::*;

pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

fn ns(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}

pub fn neopixel(rgb: RGB, tx: &mut TxRmtDriver) -> Result<()> {
    // e.g. rgb: (1,2,4)
    // G        R        B
    // 7      0 7      0 7      0
    // 00000010 00000001 00000100
    let color: u32 = ((rgb.g as u32) << 16) | ((rgb.r as u32) << 8) | rgb.b as u32;
    let ticks_hz = tx.counter_clock()?;
    let t0h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(350))?;
    let t0l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(800))?;
    let t1h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(700))?;
    let t1l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(600))?;
    let mut signal = FixedLengthSignal::<24>::new();
    for i in (0..24).rev() {
        let p = 2_u32.pow(i);
        let bit = p & color != 0;
        let (high_pulse, low_pulse) = if bit {
            (t1h, t1l)
        } else {
            (t0h, t0l)
        };
        signal.set(23 - i as usize, &(high_pulse, low_pulse))?;
    }
    tx.start_blocking(&signal)?;

    Ok(())
}

/// Converts hue, saturation, value to RGB
pub fn hsv2rgb(h: u32, s: u32, v: u32) -> Result<RGB> {
    if h > 360 || s > 100 || v > 100 {
        bail!("The given HSV values are not in valid range");
    }
    let s = s as f64 / 100.0;
    let v = v as f64 / 100.0;
    let c = s * v;
    let x = c * (1.0 - (((h as f64 / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b);
    if h < 60 {
        r = c;
        g = x;
        b = 0.0;
    } else if h >= 60 && h < 120 {
        r = x;
        g = c;
        b = 0.0;
    } else if h >= 120 && h < 180 {
        r = 0.0;
        g = c;
        b = x;
    } else if h >= 180 && h < 240 {
        r = 0.0;
        g = x;
        b = c;
    } else if h >= 240 && h < 300 {
        r = x;
        g = 0.0;
        b = c;
    } else {
        r = c;
        g = 0.0;
        b = x;
    }

    Ok(RGB {
        r: ((r + m) * 255.0) as u8,
        g: ((g + m) * 255.0) as u8,
        b: ((b + m) * 255.0) as u8,
    })
}
