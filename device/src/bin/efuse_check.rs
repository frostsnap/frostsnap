#![no_std]
#![no_main]

extern crate alloc;
use alloc::string::String;
use core::fmt::Write as _;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_hal::{entry, hmac::KeyId, timer::Timer as _};
use frostsnap_device::{
    efuse::{EfuseController, EfuseHmacKeys},
    peripherals::DevicePeripherals,
    touch_handler, DISPLAY_REFRESH_MS,
};
use frostsnap_widgets::{
    debug::{EnabledDebug, OverlayDebug},
    scrollable_debug_text::ScrollableDebugText,
    DynWidget, Padding, Widget,
};

const TEST_MESSAGE: &[u8] = b"efuse_check_test";

const KEY_IDS: [KeyId; 6] = [
    KeyId::Key0,
    KeyId::Key1,
    KeyId::Key2,
    KeyId::Key3,
    KeyId::Key4,
    KeyId::Key5,
];

#[entry]
fn main() -> ! {
    esp_alloc::heap_allocator!(256 * 1024);

    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = esp_hal::clock::CpuClock::max();
        config
    });

    let device = DevicePeripherals::init(peripherals);
    let DevicePeripherals {
        display,
        efuse,
        hmac,
        mut touch_receiver,
        timer,
        ..
    } = *device;

    let wr_dis = efuse.read_wr_dis();
    let rd_dis = efuse.read_rd_dis();

    let mut output = String::new();

    writeln!(output, "=== eFuse Key Slots ===").unwrap();
    writeln!(output, "WR_DIS: 0x{:08X}  RD_DIS: 0x{:02X}", wr_dis, rd_dis).unwrap();

    for (i, &key_id) in KEY_IDS.iter().enumerate() {
        let purpose = EfuseController::key_purpose_pub(key_id);
        let key_wr = (wr_dis >> (23 + i)) & 1 == 1;
        let kp_wr = (wr_dis >> (8 + i)) & 1 == 1;
        let key_rd = (rd_dis >> i) & 1 == 1;

        writeln!(output).unwrap();
        writeln!(output, "KEY{}: {:?}", i, purpose).unwrap();
        writeln!(
            output,
            "  wr: data={} purpose={}",
            if key_wr { "LOCKED" } else { "open" },
            if kp_wr { "LOCKED" } else { "open" },
        )
        .unwrap();
        writeln!(
            output,
            "  rd: {}",
            if key_rd { "PROTECTED" } else { "readable" },
        )
        .unwrap();

        match efuse.read_efuse(key_id) {
            Ok(data) => {
                let all_zero = data.iter().all(|&b| b == 0);
                let all_ff = data.iter().all(|&b| b == 0xFF);
                let status = if all_zero {
                    "ALL-ZERO"
                } else if all_ff {
                    "ALL-FF"
                } else {
                    "has data"
                };
                writeln!(
                    output,
                    "  data: {} [{:02x}{:02x}{:02x}{:02x}...]",
                    status, data[0], data[1], data[2], data[3]
                )
                .unwrap();
            }
            Err(e) => {
                writeln!(output, "  data: err {:?}", e).unwrap();
            }
        }
    }

    // HMAC fingerprint + read-protect check
    writeln!(output, "\n=== HMAC Fingerprint ===").unwrap();
    let hw_hmac = match EfuseHmacKeys::load(&efuse, hmac) {
        Ok(mut keys) => {
            match keys.share_encryption.hash("efuse_check", TEST_MESSAGE) {
                Ok(h) => {
                    write!(output, "hw:  ").unwrap();
                    for b in &h[..8] {
                        write!(output, "{:02x}", b).unwrap();
                    }
                    writeln!(output).unwrap();
                    Some(h)
                }
                Err(e) => {
                    writeln!(output, "hw err: {:?}", e).unwrap();
                    None
                }
            }
        }
        Err(e) => {
            writeln!(output, "load err: {:?}", e).unwrap();
            None
        }
    };

    // Software HMAC using raw efuse bytes to test read protection
    writeln!(output, "\n=== Read Protect Check ===").unwrap();
    {
        use sha2::{Digest, Sha256};

        let discovered = efuse.discover_efuses();
        if let Some(enc_key_id) = discovered.share_encryption {
            match efuse.read_efuse(enc_key_id) {
                Ok(key_bytes) => {
                    let mut ipad = [0x36u8; 64];
                    let mut opad = [0x5cu8; 64];
                    for i in 0..32 {
                        ipad[i] ^= key_bytes[i];
                        opad[i] ^= key_bytes[i];
                    }

                    let domain = "efuse_check";
                    let len_byte = [domain.len() as u8];

                    let inner = {
                        let mut h = Sha256::new();
                        h.update(&ipad);
                        h.update(&len_byte);
                        h.update(domain.as_bytes());
                        h.update(TEST_MESSAGE);
                        h.finalize()
                    };
                    let sw = {
                        let mut h = Sha256::new();
                        h.update(&opad);
                        h.update(&inner);
                        h.finalize()
                    };

                    write!(output, "sw:  ").unwrap();
                    for b in &sw[..8] {
                        write!(output, "{:02x}", b).unwrap();
                    }
                    writeln!(output).unwrap();

                    if let Some(hw) = hw_hmac {
                        if sw[..] == hw[..] {
                            writeln!(output, "MATCH - key readable!").unwrap();
                        } else {
                            writeln!(output, "DIFFER - key protected").unwrap();
                        }
                    }
                }
                Err(e) => writeln!(output, "read err: {:?}", e).unwrap(),
            }
        } else {
            writeln!(output, "no enc key found").unwrap();
        }
    }

    // Secure boot status
    writeln!(output, "\n=== Secure Boot ===").unwrap();
    let sb_enabled = frostsnap_device::secure_boot::is_secure_boot_enabled();
    writeln!(output, "enabled: {}", sb_enabled).unwrap();

    // Display with scrollable widget + padding
    let screen_size = display.size();
    let content_width = screen_size.width - 10;
    let inner = ScrollableDebugText::new(&output, content_width);
    let padded = Padding::symmetric(5, 20, inner);

    let debug_config = EnabledDebug::default();
    let mut widget = OverlayDebug::new(padded, debug_config);

    let mut display = frostsnap_widgets::SuperDrawTarget::new(
        display,
        Rgb565::BLACK,
    );

    widget.set_constraints(display.bounding_box().size);

    let mut last_touch: Option<embedded_graphics::geometry::Point> = None;
    let mut current_widget_index = 0usize;
    let mut last_redraw_time = timer.now();

    let now_ms = frostsnap_widgets::Instant::from_millis(
        timer.now().duration_since_epoch().to_millis(),
    );
    let _ = widget.draw(&mut display, now_ms);

    loop {
        let now = timer.now();
        let now_ms = frostsnap_widgets::Instant::from_millis(
            now.duration_since_epoch().to_millis(),
        );

        touch_handler::process_all_touch_events(
            &mut touch_receiver,
            &mut widget,
            &mut last_touch,
            &mut current_widget_index,
            now_ms,
        );

        let elapsed_ms = (now - last_redraw_time).to_millis();
        if elapsed_ms >= DISPLAY_REFRESH_MS {
            last_redraw_time = now;
            let _ = widget.draw(&mut display, now_ms);
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    frostsnap_device::panic::handle_panic(info)
}
